//! The operating-system glue.
//!
//! Every `unsafe` line in Safe Invest is in this crate, and each one wraps a C
//! API that has no safe equivalent in the dependency tree: DPAPI for sealing an
//! API key, and console attachment so a windowed executable can still answer
//! `--version` at a prompt.
//!
//! Keeping them together buys two things. The security review has one small
//! file to read rather than a hunt through five crates. And because this crate
//! depends on nothing but `windows-sys`, it can be type-checked against the
//! Windows target from a Linux machine — everything else in the workspace pulls
//! in `ring`, whose build script cannot cross-compile to MSVC, so a mistake in
//! a Win32 signature would otherwise only surface minutes into a Windows CI
//! job.

pub mod console;
pub mod secret;
