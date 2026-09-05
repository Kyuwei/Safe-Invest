//! What the executable does when it has nowhere to print.
//!
//! The Windows release build is a GUI-subsystem executable: started without a
//! terminal, it has *null* standard handles. `println!` panics when a write
//! fails, and under `panic = "abort"` that is a bare non-zero exit code with
//! nothing printed — which is how `safe-invest.exe --version` first failed its
//! own smoke test on CI.
//!
//! There is no null handle on Linux, but `/dev/full` fails every write in the
//! same way, so the regression is testable here.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]

use std::process::{Command, Stdio};

fn run_with_unwritable_stdout(args: &[&str]) -> std::process::ExitStatus {
    let sink = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full doit exister sur cette plateforme");

    Command::new(env!("CARGO_BIN_EXE_safe-invest"))
        .args(args)
        .stdout(Stdio::from(sink))
        .stderr(Stdio::null())
        .status()
        .expect("le binaire doit démarrer")
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "/dev/full est propre à Linux")]
fn version_succeeds_even_when_it_cannot_be_printed() {
    let status = run_with_unwritable_stdout(&["--version"]);
    assert!(
        status.success(),
        "une sortie standard impossible à écrire ne doit pas faire échouer --version (code {:?})",
        status.code()
    );
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "/dev/full est propre à Linux")]
fn help_succeeds_even_when_it_cannot_be_printed() {
    assert!(run_with_unwritable_stdout(&["--help"]).success());
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "/dev/full est propre à Linux")]
fn doctor_succeeds_even_when_it_cannot_be_printed() {
    let dir = tempfile::tempdir().unwrap();
    let status = run_with_unwritable_stdout(&[
        "doctor",
        "--demo",
        "--data-dir",
        dir.path().to_str().unwrap(),
    ]);
    assert!(status.success(), "code {:?}", status.code());
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "/dev/full est propre à Linux")]
fn a_bad_argument_still_reports_a_failure() {
    // The point is not that everything succeeds — it is that the exit code
    // means what it says. A real error must still be an error.
    let status = run_with_unwritable_stdout(&["--turbo"]);
    assert_eq!(
        status.code(),
        Some(2),
        "une option inconnue reste une erreur"
    );
}
