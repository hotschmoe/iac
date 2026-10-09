// iac-sim must stay a pure function of its inputs: no wall clock, no OS
// entropy, no I/O, no async runtime. This scans the library source (the
// `bin/` runner is a host and may time itself) and the manifest.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_SOURCE: &[&str] = &[
    "std::time",
    "SystemTime",
    "Instant::now",
    "std::thread",
    "std::fs",
    "std::net",
    "std::env",
    "thread_rng",
    "rand::rng(",
    "OsRng",
    "getrandom",
    "tokio",
    "rusqlite",
    "axum",
];

const FORBIDDEN_DEPENDENCIES: &[&str] = &["tokio", "axum", "rusqlite", "getrandom", "chrono", "time"];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "bin") {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_library_source_never_reads_a_clock_or_does_io() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    assert!(files.len() > 5, "found the source tree");

    let mut hits = Vec::new();
    for file in files {
        for (n, line) in fs::read_to_string(&file).unwrap().lines().enumerate() {
            for token in FORBIDDEN_SOURCE {
                if line.contains(token) {
                    hits.push(format!("{}:{}: {token}", file.strip_prefix(root).unwrap().display(), n + 1));
                }
            }
        }
    }
    assert!(hits.is_empty(), "iac-sim must stay pure:\n{}", hits.join("\n"));
}

#[test]
fn the_manifest_pulls_in_no_io_runtime() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let deps = manifest.split("[dependencies]").nth(1).expect("a dependencies table");
    let deps = deps.split("\n[").next().unwrap();
    for name in FORBIDDEN_DEPENDENCIES {
        assert!(
            !deps.lines().any(|l| l.split('=').next().is_some_and(|k| k.trim() == *name)),
            "iac-sim depends on {name}"
        );
    }
}
