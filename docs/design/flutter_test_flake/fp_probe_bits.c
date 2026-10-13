#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <sched.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdatomic.h>
#include <sys/prctl.h>
#include <time.h>
#include <unistd.h>

#ifndef PR_SVE_GET_VL
#define PR_SVE_GET_VL 51
#endif
#define PR_SVE_VL_LEN_MASK 0xffff

static atomic_ullong fail_scalar, fail_neon, fail_sve, fail_mem, iters, sigs;
static atomic_ullong fail_sum, fail_quot, fail_prod;
static atomic_ullong fail_by[6];
static int skip_sve;
static int int_handler;
static int stay;
static uint64_t sample_bits[8];
static uint64_t sample_d8[8];
static uint64_t sample_nz[8];
static int sample_kind[8];
static atomic_int sample_n;

static void sample(int kind, double x) {
  uint64_t u;
  memcpy(&u, &x, 8);
  int n = atomic_fetch_add(&sample_n, 1);
  if (n >= 0 && n < 8) {
    sample_bits[n] = u;
    sample_kind[n] = kind;
  }
}
static atomic_int stop;
static uint64_t* mem;
static size_t mem_words;
static int hop[16] = {0, 2, 1, 3, 6, 4, 7, 5, 8, 2, 9, 3, 10, 4, 11, 5};
static int hop_n = 16;

static void on_sig(int sig) {
  (void)sig;
  if (!int_handler) {
    volatile double x = 1.25;
    for (int i = 0; i < 8; i++) x = x * 1.125 + 0.5;
    (void)x;
  }
  atomic_fetch_add(&sigs, 1);
}

static inline __attribute__((always_inline)) int scalar_once(void) {
  volatile double one = 1.0;
  double s = 0;
  for (int i = 0; i < 1000; i++) s += one;
  if (s != 1000.0) {
    uint64_t nz, raw8;
    asm volatile("mrs %0, nzcv\n\tfmov %1, d8\n" : "=r"(nz), "=r"(raw8));
    int n = atomic_fetch_add(&sample_n, 1);
    if (n >= 0 && n < 8) {
      uint64_t u;
      memcpy(&u, &s, 8);
      sample_bits[n] = u;
      sample_d8[n] = raw8;
      sample_nz[n] = nz;
      sample_kind[n] = 1;
    }
    atomic_fetch_add(&fail_sum, 1);
    return 1;
  }
  volatile double three = 3.0;
  double q = three / 0.5;
  if (q != 6.0) {
    sample(2, q);
    atomic_fetch_add(&fail_quot, 1);
    return 1;
  }
  double p = 1.0;
  for (int i = 0; i < 10; i++) p *= 2.0;
  if (p != 1024.0) {
    sample(3, p);
    atomic_fetch_add(&fail_prod, 1);
    return 1;
  }
  return 0;
}

static inline __attribute__((always_inline)) int neon_once(void) {
  uint64_t in[4] = {
      0x3ff0000000000001ULL, 0x4000000000000002ULL, 0x4008000000000003ULL,
      0x4010000000000004ULL};
  uint64_t out[4];
  uint64_t spins = 8000;
  asm volatile(
      "ldr q0, [%[in]]\n"
      "ldr q1, [%[in], #16]\n"
      "1:\n"
      "subs %[spins], %[spins], #1\n"
      "b.ne 1b\n"
      "str q0, [%[out]]\n"
      "str q1, [%[out], #16]\n"
      : [spins] "+r"(spins)
      : [in] "r"(in), [out] "r"(out)
      : "memory", "cc", "v0", "v1");
  return memcmp(in, out, sizeof(in)) != 0;
}

static inline __attribute__((always_inline)) int sve_once(void) {
  uint64_t out[2];
  uint64_t spins = 8000;
  asm volatile(
      "ptrue p0.d\n"
      "index z0.d, #0, #1\n"
      "1:\n"
      "subs %[spins], %[spins], #1\n"
      "b.ne 1b\n"
      "str z0, [%[out]]\n"
      : [spins] "+r"(spins)
      : [out] "r"(out)
      : "memory", "cc", "z0", "p0");
  return out[0] != 0 || out[1] != 1;
}

static inline __attribute__((always_inline)) int mem_once(uint64_t seed) {
  size_t n = mem_words;
  uint64_t* p = mem;
  for (size_t i = 0; i < 4096; i++) {
    size_t j = (seed + i * 1315423911u) % n;
    uint64_t expect = 0xA5A5A5A5A5A5A5A5ULL ^ (uint64_t)j * 0x9E3779B97F4A7C15ULL;
    if (p[j] != expect) return 1;
  }
  return 0;
}

static void* worker(void* arg) {
  int id = (int)(intptr_t)arg;
  while (!atomic_load(&stop)) {
    if (scalar_once()) {
      atomic_fetch_add(&fail_scalar, 1);
      atomic_fetch_add(&fail_by[id], 1);
    }
    if (neon_once()) atomic_fetch_add(&fail_neon, 1);
    if (!skip_sve && sve_once()) atomic_fetch_add(&fail_sve, 1);
    if (mem_once((uint64_t)id)) atomic_fetch_add(&fail_mem, 1);
    atomic_fetch_add(&iters, 1);
    if (!stay && (id & 1) == 0) {
      cpu_set_t set;
      CPU_ZERO(&set);
      CPU_SET(hop[(atomic_load(&iters) + id) % hop_n], &set);
      sched_setaffinity(0, sizeof(set), &set);
    }
  }
  return NULL;
}

static void* signaler(void* arg) {
  pthread_t* threads = arg;
  int n = 6;
  while (!atomic_load(&stop)) {
    for (int i = 0; i < n; i++) pthread_kill(threads[i], SIGUSR1);
    usleep(1000);
  }
  return NULL;
}

static void fill_mem(void) {
  mem_words = (8 * 1024 * 1024) / sizeof(uint64_t);
  mem = malloc(mem_words * sizeof(uint64_t));
  if (!mem) {
    perror("malloc");
    exit(1);
  }
  for (size_t j = 0; j < mem_words; j++) {
    mem[j] = 0xA5A5A5A5A5A5A5A5ULL ^ (uint64_t)j * 0x9E3779B97F4A7C15ULL;
  }
}

static void report_vl(void) {
  cpu_set_t saved;
  sched_getaffinity(0, sizeof(saved), &saved);
  printf("affinity");
  for (int cpu = 0; cpu < 12; cpu++)
    if (CPU_ISSET(cpu, &saved)) printf(" %d", cpu);
  printf("\n");
  for (int cpu = 0; cpu < 12; cpu++) {
    if (!CPU_ISSET(cpu, &saved)) continue;
    cpu_set_t set;
    CPU_ZERO(&set);
    CPU_SET(cpu, &set);
    if (sched_setaffinity(0, sizeof(set), &set) != 0) {
      printf("cpu %d affinity errno %d\n", cpu, errno);
      continue;
    }
    int vl = prctl(PR_SVE_GET_VL);
    char midr_path[80];
    snprintf(midr_path, sizeof(midr_path),
             "/sys/devices/system/cpu/cpu%d/regs/identification/midr_el1", cpu);
    FILE* f = fopen(midr_path, "r");
    char midr[64] = "?";
    if (f) {
      if (!fgets(midr, sizeof(midr), f)) strcpy(midr, "?");
      fclose(f);
      midr[strcspn(midr, "\n")] = 0;
    }
    if (vl < 0)
      printf("cpu %d midr %s sve prctl errno %d\n", cpu, midr, errno);
    else
      printf("cpu %d midr %s sve_vl_bytes %d\n", cpu, midr, vl & PR_SVE_VL_LEN_MASK);
  }
  sched_setaffinity(0, sizeof(saved), &saved);
  fflush(stdout);
}

static void start_workers(pthread_t* threads) {
  atomic_store(&stop, 0);
  for (int i = 0; i < 6; i++) pthread_create(&threads[i], NULL, worker, (void*)(intptr_t)i);
}

static void run_chunk(const char* name, int seconds, pthread_t* threads, int with_sig) {
  atomic_store(&fail_scalar, 0);
  atomic_store(&fail_neon, 0);
  atomic_store(&fail_sve, 0);
  atomic_store(&fail_mem, 0);
  atomic_store(&fail_sum, 0);
  atomic_store(&fail_quot, 0);
  atomic_store(&fail_prod, 0);
  atomic_store(&sample_n, 0);
  for (int i = 0; i < 6; i++) atomic_store(&fail_by[i], 0);
  atomic_store(&iters, 0);
  atomic_store(&sigs, 0);
  start_workers(threads);
  pthread_t sig_thread;
  if (with_sig) pthread_create(&sig_thread, NULL, signaler, threads);
  sleep(seconds);
  atomic_store(&stop, 1);
  if (with_sig) pthread_join(sig_thread, NULL);
  for (int i = 0; i < 6; i++) pthread_join(threads[i], NULL);
  printf(
      "CHUNK %s scalar %llu neon %llu sve %llu mem %llu iters %llu sigs %llu sum %llu quot %llu prod %llu\n",
      name, (unsigned long long)fail_scalar, (unsigned long long)fail_neon,
      (unsigned long long)fail_sve, (unsigned long long)fail_mem,
      (unsigned long long)iters, (unsigned long long)sigs,
      (unsigned long long)fail_sum, (unsigned long long)fail_quot,
      (unsigned long long)fail_prod);
  printf("BYID");
  for (int i = 0; i < 6; i++)
    printf(" %llu", (unsigned long long)fail_by[i]);
  printf("\n");
  int n = atomic_load(&sample_n);
  if (n > 8) n = 8;
  for (int i = 0; i < n; i++) {
    printf("SAMPLE kind %d d0 %llx d8 %llx nzcv %llx\n", sample_kind[i],
           (unsigned long long)sample_bits[i],
           (unsigned long long)sample_d8[i],
           (unsigned long long)sample_nz[i]);
  }
  fflush(stdout);
}

int main(int argc, char** argv) {
  const char* mode = argc > 1 ? argv[1] : "cross";
  if (strcmp(mode, "big") == 0) {
    int b[] = {0, 1, 6, 7, 8, 9, 10, 11};
    memcpy(hop, b, sizeof(b));
    hop_n = 8;
  } else if (strcmp(mode, "little") == 0) {
    int b[] = {2, 3, 4, 5};
    memcpy(hop, b, sizeof(b));
    hop_n = 4;
  } else if (strcmp(mode, "same") == 0) {
    hop[0] = 0;
    hop_n = 1;
  } else if (strcmp(mode, "nosve") == 0) {
    skip_sve = 1;
  } else if (strcmp(mode, "int") == 0) {
    int_handler = 1;
  } else if (strcmp(mode, "stay") == 0) {
    stay = 1;
  }
  int seconds = 3;
  if (argc > 2) seconds = atoi(argv[2]);
  if (seconds < 1) seconds = 1;
  struct sigaction sa;
  memset(&sa, 0, sizeof(sa));
  sa.sa_handler = on_sig;
  sigemptyset(&sa.sa_mask);
  sa.sa_flags = SA_RESTART;
  sigaction(SIGUSR1, &sa, NULL);
  fill_mem();
  report_vl();
  pthread_t threads[6];
  run_chunk(mode, seconds, threads, 1);
  printf("DONE\n");
  return 0;
}
