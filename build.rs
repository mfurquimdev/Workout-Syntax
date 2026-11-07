//! The owner's live log is not part of the repository. Tests read
//! `$OUT_DIR/live.md`: a copy of `Workout.md` at the crate root (or of the
//! file named by `WORKOUT_LOG`) when present, otherwise of the test fixture
//! `tests/fixtures/Workout.md`, so the tests also run on a fresh clone.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let live = std::env::var("WORKOUT_LOG").map(PathBuf::from).unwrap_or_else(|_| root.join("Workout.md"));
    let fixture = root.join("tests/fixtures/Workout.md");
    println!("cargo::rerun-if-env-changed=WORKOUT_LOG");
    println!("cargo::rerun-if-changed={}", live.display());
    println!("cargo::rerun-if-changed={}", fixture.display());
    println!("cargo::rustc-check-cfg=cfg(live_log)");
    let source = if live.is_file() {
        // tests that compare the live log with the older fixture need it
        println!("cargo::rustc-cfg=live_log");
        live
    } else {
        fixture
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("live.md");
    std::fs::copy(&source, &out).expect("copy the log for the tests");
}
