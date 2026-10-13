# Flutter tests flake on this machine

Date: 2026-10-13. Status: measurements, for the owner. Nothing in the client
was changed. The suite is not made reliable by anything tried here.

The widget tests in `clients/web/test` (479 tests, about 35 seconds on the
big cores) fail on a fraction of full runs. The failing test is a different
one each time. The values are not ones the client's code can compute from
its inputs: `Unsupported operation: Infinity or NaN toInt` out of
`StateMapper._mapFleet` and out of `DemoWorld._ticksFor`, a `RenderFlex`
thousands of pixels too big, assertions inside Flutter's own layout and text
code, and once a `flutter_tester` process that sat at full CPU for 2 hours
20 minutes. The Rust simulation in the same repository, compared byte for
byte between native code and WebAssembly, has not shown this.

Machine: CIX P1, `Linux ms-r1 6.6.10-cix-build-generic #2 SMP PREEMPT Sat Oct
11 18:51:40 UTC 2025 aarch64`. Flutter 3.44.1 stable, engine `c416acfeb8`,
Dart 3.12.1, both at `~/development/flutter`. Nothing there was upgraded or
reconfigured. Cortex-A720 on CPUs 0, 1 and 6-11 (MIDR `0x410fd811`; CPUs 0-1
max 2600 MHz, CPUs 6-11 max 2500 MHz) and Cortex-A520 on CPUs 2-5 (MIDR
`0x410fd801`, max 1800 MHz). All twelve CPUs advertise `sve` and `sve2` in
`/proc/cpuinfo`. None advertises `sme`. `/proc/sys/abi/sve_default_vector_length`
is 16, and `prctl(PR_SVE_GET_VL)` is 16 bytes on every CPU. A vector-length
mismatch between the two kinds of core is ruled out.

Logs of the runs below are in
`/tmp/claude-1001/-home-hotschmoe-github-iac/fe48b2fd-512c-42dd-a4fc-cad58184bc0f/scratchpad/grok/flakes/`
unless a name says otherwise. The earlier rate logs (`ftest-client*.log`,
`rate-merged-*.log`, `rate-master-*.log`, `rate-f32-*.log`) are in the parent
of that directory and were not rewritten.

## What was run

Every comparison that this session started was interleaved (one arm, the
other arm, repeat), not blocked. At most two heavy processes at a time,
`nice -n 10`, and every `flutter test` under `timeout -k 20 300`.

### The suite, one A720 against six A720s

Checkout `clients/web` at `7289881` (the client is the merge `06aa24a`).
Eight pairs. Each pair ran the full suite twice at once:

- `taskset -c 0` (one A720), log `ft-one-NN.log`
- `taskset -c 6-11` (the scheduler may move threads among those six), log
  `ft-mig-NN.log`

Both arms were prefixed with `DART_VM_OPTIONS=--profile_period=100000000`.
That variable does not reach the test VM. See below. The runner is
`run_ft.sh`; its summary is `ft-runner.log`. Pair 01 also sampled the live
`flutter_tester` processes into `ft-mig-01-procs.log`.

| run | `taskset -c 0` | `taskset -c 6-11` |
|---|---|---|
| 01 | pass, 479, 2:46 | pass, 479, 0:41 |
| 02 | pass, 479, 2:51 | pass, 479, 0:37 |
| 03 | fail, 478, 2:55 | pass, 479, 0:40 |
| 04 | fail, 478, 2:51 | pass, 479, 0:36 |
| 05 | pass, 479, 2:51 | pass, 479, 0:36 |
| 06 | pass, 479, 2:39 | pass, 479, 0:36 |
| 07 | pass, 479, 2:49 | pass, 479, 0:38 |
| 08 | fail, 478, 2:49 | pass, 479, 0:37 |

One core: 3 failed of 8. CPUs 6-11: 0 failed of 8.

The three failures:

- `ft-one-03.log`, `map_test.dart`, "route holds before a T5+ sector". A
  `RenderFlex` overflowed by 793 pixels on the bottom (`hud.dart:239`, a
  vertical `Column` whose height constraint is at most 55 and whose size was
  reported as `Size(2983.0, 55.0)`) and by 2065 pixels on the right
  (`hud.dart:54`, a `Row` whose width constraint is at most 1440).
- `ft-one-04.log`, `onboarding_test.dart:188`. The test expects no exception
  and got `A RenderFlex overflowed by 15 pixels on the bottom`. Fifteen
  pixels is the same exception class. It is not the thousands-of-pixels
  signature.
- `ft-one-08.log`, `outpost_test.dart:400`, the scripted-world demo.
  `Unsupported operation: Infinity or NaN toInt` from `double.ceil` in
  `DemoWorld._ticksFor` (`demo_world.dart:2089`). The expression is
  `(cost.metal + cost.crystal) / rate * 3600 / speed / _pace / _lapse`, and
  those inputs are constants in the source.

The one-core runs took about four times as long, so they took more profiler
samples if a failure is per sample. 3 of 8 against 0 of 8 is the count of
runs. It does not show that one core is worse per second, and 0 of 8 does
not overturn the earlier rate on the wider big set.

`ft-mig-01-procs.log`: every `flutter_tester` had `Cpus_allowed_list: 6-11`,
caught signals `SigCgt: 0000000104000000` (SIGPROF, signal 27, and signal
33), and had a thread named `Dart Profiler T` plus `DartWorker`,
`dart:io EventHa`, and `io.worker.1` through `io.worker.4`. The CPUs of
those threads hopped across 6-11 inside a single process. The profiler was
on for the whole pair.

This session did not run the suite on CPUs 2-5, did not run
`--concurrency=1`, and did not run `taskset -c 0,1,6-11`.

### Earlier suite rates, not remeasured here

These are the runs already on disk from 2026-10-11 and 2026-10-13, all of
the later day under `DART_VM_OPTIONS=--profile_period=100000000` and
`taskset -c 0,1,6-11`. They were not repeated as an interleaved comparison
in this session.

| tree | pin | failed / runs |
|---|---|---|
| 2026-10-11, unpinned | none | 4 / 6 |
| 2026-10-11, A720s | `0,1,6-11` | 0 / 6, then later under load 2 / 5 |
| client of `cc9cebb` (`master-tree/`) | `0,1,6-11` | 1 / 13 |
| this client, `06aa24a` | `0,1,6-11` | 9 / 25 (alone 3 / 9, two suites at once 6 / 16) |
| same tree, `_f32` rewritten over `ByteData` (`f32-tree/`) | `0,1,6-11` | 2 / 8 |

The signatures in those logs are the ones in the opening paragraph, plus a
test process that would not start, `fontSizeFactor == 1.0` inside
`TextStyle.apply` under `new ThemeData` failing every test of one file (one
poisoned process, not a dozen independent faults), `debugAssertIsValid()` in
`box.dart`, `debugSize == size` in `text_painter.dart`, `RenderBox.size`
during `performLayout`, and "Offset/Rect argument contained a NaN" from the
windshield painter and from `MapViewport.toHex` on finite pointer
coordinates.

### A C program, no Dart

`docs/design/flutter_test_flake/fp_probe_bits.c` is the program. Six threads.
Each iteration sums one thousand `1.0`s and compares the sum to `1000.0`
held in callee-saved `d8` for the whole worker, divides `3.0 / 0.5`,
multiplies ten `2.0`s, spins NEON `q0`/`q1` and an SVE `index` for 8000
trips, and checks 4096 words of an 8 MB buffer. A seventh thread sends
`SIGUSR1` at about 1 kHz via `pthread_kill`. Even thread ids call
`sched_setaffinity` every iteration, except in mode `stay`, which never
calls it. Build:

```sh
gcc -O2 -Wall -Wextra -march=armv8.2-a+sve -o fp_probe_bits fp_probe_bits.c -pthread
```

`-fno-inline` on a copy of the same source did not fail (below). The SVE
asm has to be inlined into the worker so that `d8` stays live across it.

On a failing compare the first instructions of the taken path are
`mrs nzcv` and `fmov d8`. Every sample this program printed is the same:
the sum in `d0` is `0x408f400000000000` (exactly 1000.0), `d8` is 0, and
NZCV is `0x20000000` (the compare "greater than", which is what `fcmp` of
1000.0 against +0.0 must report). The sum was computed correctly. The
callee-saved register was read back as +0.0. NEON, the SVE register check,
and the memory check stayed at 0 in every failing run. Only `d8` was
sampled. `d9` to `d15` were not.

Modes, one run of 3 seconds unless noted. Logs `fp-bits-big.log`,
`fp-bits-little.log`, `fp-bits-cross.log`, `fp-bits-same.log`. The `int` and
`nosve` runs were printed to the session terminal and are quoted here.

| mode | what it changes | scalar fails | signals | who failed |
|---|---|---|---|---|
| `big` | hop CPUs 0, 1, 6-11 | 11313 | 10521 | even threads only, 3787 / 0 / 3751 / 0 / 3775 / 0 |
| `little` | hop CPUs 2-5 | 18475 | 16611 | even threads only, 6359 / 0 / 6046 / 0 / 6070 / 0 |
| `cross` (default) | hop A720 and A520 | 12968 | 13534 | even threads only, 4480 / 0 / 4310 / 0 / 4178 / 0 |
| `int` | handler does no floating point | 12989 | 13541 | even threads only, 4303 / 0 / 4306 / 0 / 4380 / 0 |
| `nosve` | the SVE asm is skipped | 0 | 13629 | nobody |
| `same` | `sched_setaffinity` to CPU 0 every time | 59726 | 11308 | thread 2 only. One run |

`big` fails on A720s alone and `little` fails on A520s alone. The two kinds
of core are not the split.

The same binary was then run as four rounds of three arms, in that order,
12 seconds for `stay` and 4 seconds for the `big` control (`dart-runner.log`,
`fp-stay-one-NN.log`, `fp-stay-mig-NN.log`, `fp-big-ctrl-NN.log`):

| round | `stay` on CPU 0 | `stay` on CPUs 6-11 | `big` control |
|---|---|---|---|
| 01 | 0 fails, 10168 signals | 0 / 59638 | 67974 fails, 16473 signals, even threads only |
| 02 | 0 / 10819 | 0 / 64766 | 16370 / 15111, even threads only |
| 03 | 0 / 10770 | 0 / 48987 | 73931 / 16699, even threads only |
| 04 | 0 / 10672 | 0 / 59377 | 67686 / 16275, even threads only |

`stay` means the workers never call `sched_setaffinity`. On CPU 0 the six
threads preempt each other. On CPUs 6-11 the kernel may migrate them, and
the affinity line in each log is `6 7 8 9 10 11`. Across those eight runs,
about 42 thousand signals on one CPU and about 233 thousand on six CPUs,
the scalar check failed 0 times. The control beside them, which does call
`sched_setaffinity`, failed every time. Round 02 of the control failed less
hard (16370 against about 68 thousand in the other three) and still only on
the threads that hop.

So, in this sample, the corruption shows up when the thread has executed
SVE, calls `sched_setaffinity`, and takes a signal. Dropping any one of
those three, in the runs above, dropped the failures to zero. Preemption on
one CPU, and migration that the scheduler does by itself, did not produce a
single bad compare in 12 seconds times four.

Other programs, same question, scratch directory only:

- `fp_probe` (the first program of this shape), `fp-probe-rerun.log`, four
  rounds on all twelve CPUs. No signal: scalar 0, about 1.5 million
  iterations a round. With signals: scalar 34504, 34547, 35615, 35125
  against 36034, 35963, 36269, 36153 signals. NEON, SVE and memory stayed 0.
  `fp-probe-mixed.log` is the same shape and the same result.
- `fp_probe_noinline`, the bits program built `-O2 -fno-inline`. One signal
  run: scalar 0, 16736 signals, 734683 iterations. The compiler reloads
  1000.0 from an immediate in a separate function, and that does not fail.
- `fp_shape`, ten phases of a sum checked in `d8`, with and without SVE,
  with and without hopping. `fp-shape.log`: 0 failures in every phase.
  Shape alone is not enough. The value has to stay in `d8` across the SVE
  instruction.
- `fp_flags`. The handler is asm that sets Z and returns. A tight `fcmp` of
  two freshly loaded 1000.0s, the same after `ptrue`/`index`, and a
  thousand-add that reloads `d8` every call: 0 failures, 7104 to 7400
  signals, tens of millions of compares. General NZCV restore works. "Any
  signal corrupts the flags" and "any `fcmp` lies" are false.
- `fp_sig` and `fp_sig_sve`: a four-thread sum. They did not fail. The SVE
  build never actually executed SVE in the loop that held the sum.

`which_memcpy` resolved `memcpy` to glibc offset `0x9d340`. That code is
`cntb`, `whilelo`, `ld1b {z0.b}`: the SVE memcpy. `memcpy@@GLIBC_2.17` is an
ifunc. `flutter_tester` and `dartvm` both import `memcpy`. A thread that has
copied memory on this machine has executed SVE.

### Pure Dart, JIT and AOT

`dart_fp.dart` in the scratch directory. Four isolates, 5 seconds each, sums,
products, `round`, a hex rounding, a power, and a map of records. It checks
itself. Four rounds, each round in this order, the two pins of an arm at the
same time. Logs `dart-<arm>-<pin>-NN.log`, summary `dart-runner.log`. Arms:
`dart run` with no flags, `dart --optimization_counter_threshold=-1 run`,
`dart --no_background_compilation run`, and `dart compile exe`. Flags were
command-line flags. The smoke run that passed both JIT flags is
`dart-flag-smoke.log`. The unoptimized arm did about a tenth as many checks
on one core (about 200 thousand against about 1.8 million), so the flag took
effect.

| arm | CPU 0, 4 runs | CPUs 6-11, 4 runs |
|---|---|---|
| JIT, default | 0 fails | 0 fails |
| JIT, optimization threshold -1 | 0 fails | 0 fails |
| JIT, no background compilation | 0 fails | 0 fails |
| AOT | 0 fails | 0 fails |

32 runs, 0 failures. `dart-jit-mig-01-procs.log` and
`dart-aot-mig-01-procs.log`: no thread named `Dart Profiler T`. JIT `SigCgt`
was `000000010000045c` and AOT `0000000100000458`. Neither includes SIGPROF
(bit 26, the `0x4000000` that `flutter_tester` has). The loop did not fail.
It also did not run under the profiler, and it is not the register shape
that the C program needs. A pass here bounds this loop. It does not clear
the VM, and it does not clear the suite.

`dart` (the CLI binary) and `dartvm` do not contain the string
`DART_VM_OPTIONS`. `dartaotruntime` does. The JIT flags above were passed on
the command line, which the CLI accepted.

### Whether `DART_VM_OPTIONS` reaches `flutter_tester`

It does not.

The string `DART_VM_OPTIONS` is absent from
`~/development/flutter/bin/cache/artifacts/engine/linux-arm64/flutter_tester`
and absent from `dartvm`. It is present in `dartaotruntime`, which is not
the program `flutter test` runs. Every suite run in this investigation
exported `DART_VM_OPTIONS=--profile_period=100000000`. That did not change
the test VM. The live processes still had a `Dart Profiler T` thread and
still caught SIGPROF.

`flutter test` has no `--dart-flags` option (`commands/test.dart` never
reads one). The harness that starts the tester
(`packages/flutter_tools/lib/src/test/flutter_tester_device.dart`) builds the
command without `--dart-flags`. It does pass `--enable-dart-profiling`,
because `DebuggingOptions.enabled` defaults `enableDartProfiling` to true and
the test command never sets it false.

The engine, in a build with `NDEBUG` (the shipped tester; a debug engine
forces the profiler off), turns that switch into `--profiler` and
`--profile_period=1000` (`engine/src/flutter/runtime/dart_vm.cc`,
`ProfilingFlags`). The period is microseconds. 1000 is 1 kHz. This session
did not measure the period. It measured the thread and the signal mask.

The engine will accept a `--dart-flags=` list, comma-separated, and will
abort on anything outside its allowlist
(`engine/src/flutter/shell/common/switches.cc`). On a non-release engine the
allowlist includes `--profile_period` and does not include
`--optimization_counter_threshold`, `--no_background_compilation`,
`--deterministic`, or `--no-profiler`. `flutter run --dart-flags` is how an
app gets that list. `flutter test` never forwards it. So there is no way,
from `flutter test` as installed here, to pass a VM flag or to turn the
profiler off. Doing either means invoking `flutter_tester` on a command line
the tool does not build, or patching the tool. This session did not do
that, and the Flutter install was left as it is.

### Issue trackers

Searched the Dart SDK and Flutter issue trackers for aarch64 big.LITTLE,
A520, SVE, and heterogeneous cores, and for this failure text. No report
matches this machine. The existing `Infinity or NaN toInt` issues are
application bugs (a progress indicator, a sliver, semantics), closed as
such. Apple `arm64e` and Windows ARM64 crashes are different bugs. The
Linux 6.6 stable series has later arm64 SVE and FPSIMD signal fixes (Mark
Rutland, Mark Brown), largely about SME signal context and about merging
FPSIMD state on sigreturn. This machine does not advertise SME. Nothing
found describes callee-saved `d8` reading back as +0.0 after
`sched_setaffinity` and a signal. No newer Dart, Flutter, or kernel was
installed.

## Established

`DART_VM_OPTIONS` does not reach `flutter_tester`. The binary does not
contain the string, the test harness does not pass `--dart-flags`, and a
live test process still had the profiler thread and still caught SIGPROF
while the variable was set to a period of 1e8. The `--profile_period`
everyone has been exporting does nothing. A VM flag can be passed only by
putting `--dart-flags=` on the `flutter_tester` command line, and only for a
flag on the engine allowlist. `--profile_period` is on that list.
`--no-profiler` is not. `flutter test` never adds `--dart-flags`. The
profiler is turned on by `--enable-dart-profiling`, which `flutter test`
always passes, and the engine source then requests `--profile_period=1000`.

A C program with no Dart in it fails on this kernel. The program is
`docs/design/flutter_test_flake/fp_probe_bits.c`. After the thread has
executed SVE, a `sched_setaffinity` and a `SIGUSR1` leave callee-saved `d8`
reading as +0.0 while the value just computed in `d0` is still exactly
1000.0 and NZCV is the greater-than result of comparing them. It fails on
A720s alone and on A520s alone. It fails with a handler that touches no
floating-point state. It does not fail when SVE is skipped, when no signal
is sent, when the same source is built `-fno-inline`, or when the workers
never call `sched_setaffinity` (four 12-second runs on CPU 0 and four on
CPUs 6-11, interleaved with a `big` control that failed every time). Vector
length is 16 bytes on every CPU. The smallest reproducer is that file, built
`-O2 -march=armv8.2-a+sve` without `-fno-inline`, run as
`taskset -c 0,1,6-11 ./fp_probe_bits big`. That is what would be sent to the
kernel's maintainers. It is a bug in this 6.6.10 CIX kernel, or in the
interaction of that kernel with this glibc's SVE memcpy. It is not a bug in
the client.

The suite, this session, interleaved: `taskset -c 0` failed 3 of 8
(`ft-one-03`, `ft-one-04`, `ft-one-08`); `taskset -c 6-11` failed 0 of 8.
One core is not a cure. The 6-11 runs were about four times shorter.

`_f32` is not the cause (2 of 8 with the `ByteData` rewrite, beside the
merged tree). The merge `cc9cebb..06aa24a` changes `StateMapper._mapFleet` by
one line, `jumpTicks: f.jumpTicks`, an integer copied off the fleet. The
scripted world sets `jumpTicks` from integer ship speeds with integer
division. That line cannot produce a NaN.

## Only likely

The suite's NaN and the thousand-pixel overflows are the same register
event as the C program: a callee-saved double used as a scale or a divisor
comes back as 0, and a NaN or a huge size falls out of arithmetic the source
cannot produce. The preconditions are present in a test process in part.
glibc's `memcpy` is SVE, so Dart threads execute SVE. The tester catches
SIGPROF and runs a profiler thread. Its threads move across the CPUs the
`taskset` allows. `flutter_tester` imports `pthread_setaffinity_np` (the
standalone `dartvm` does not). Whether a test run actually calls it was not
traced. The C program did not fail, in the runs above, unless it called
`sched_setaffinity` itself. The suite did fail on a process pinned to one
CPU, which is the case the `stay` probe did not reproduce. So the link is
the best fit for the signatures and is not shown.

The 9 of 25 against 1 of 13 gap between this client and `cc9cebb` is chance
and load. It was not remeasured interleaved. The code change cannot produce
the symptom. A handful of new runs would not settle a fourfold claim, so
none were started.

A whole file dying on `fontSizeFactor == 1.0` fits a register or a JIT
constant that stays wrong for the rest of the process. One-shot NaNs fit a
register that is wrong for one compare and then reloaded. Both were already
in the earlier logs. Neither was caught in the act inside the VM.

## Still unknown

Whether the suite fails because of this kernel bug. The missing
measurements are a trace of `pthread_setaffinity_np` inside a `flutter test`
process, and a scratch `flutter_tester` started without
`--enable-dart-profiling` (profiler off) interleaved with the default
harness. The second one is the experiment worth doing next. It was not
built. It must not be done by editing `~/development/flutter`.

Whether `SIGPROF` in particular is required, or any signal. The probe used
`SIGUSR1`.

Whether `d9` to `d15` are cleared with `d8`.

Whether the kernel's own migration can cause it on a longer run than 12
seconds. Four runs at 0 is a strong negative for a bug that otherwise fails
on the order of once per signal, and a weak negative for a much rarer path.

Why round 02 of the `big` control failed about four times less often than
the rounds around it. It still failed, and only on hopping threads.

The merge rate, as above. Little-core suite runs. `--concurrency=1`.
`--deterministic` on the tester, which the allowlist would reject.

No upstream issue was found that claims to fix this symptom, so it is
unknown whether a newer kernel already has.

## What to do

Nothing measured makes the suite reliable on this machine.

`taskset -c 0` failed 3 of 8. `taskset -c 0,1,6-11` failed 9 of 25 on this
client in the earlier sample, and 2 of 5 under load on 2026-10-11.
`taskset -c 6-11` passed 8 of 8 here, on the short arm of a paired run.
That is not a basis for a new pin. `DART_VM_OPTIONS` does not affect the
run.

Run it with a timeout, keep the log, and run a failure again before reading
it as the client:

```sh
cd clients/web && timeout -k 20 300 nice -n 10 flutter test
```

There is no flag to add to that line that these measurements support.

Recommendations, not done:

- Do not add a wrapper that retries failed tests. It hides a real failure
  the day the client has one.
- Do not patch the client to dodge this. The arithmetic that throws is
  already on constants.
- Send `fp_probe_bits.c` and the `big` counts to whoever maintains
  `6.6.10-cix-build-generic`. The failure is outside this repository.
- If one more experiment is wanted: a copy of the tester command with
  `--enable-dart-profiling` left off, interleaved with the stock `flutter
  test`, same pin, same timeout. That is the split between the profiler
  signal and everything else. Leave the installed Flutter tree alone.
