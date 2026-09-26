//! Writing files that two processes share.
//!
//! The window and the MCP server write the same directory. Two rules keep that
//! safe, and both live here so the saves and the settings follow the same ones:
//! a write goes to a temporary sibling that is then renamed over the target (a
//! reader never sees half a file), and a read-modify-write holds an OS lock for
//! its whole duration (two writers never interleave).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

/// How many times a rename refused by Windows is tried again.
const RENAME_ATTEMPTS: u32 = 6;

/// Writes `bytes` so that `path` is either the old content or the new one, and
/// never a truncated mix of the two.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("chemin sans dossier parent"))?;
    fs::create_dir_all(parent)?;

    // The temporary file is a sibling: `rename` is only atomic within one
    // filesystem, and the system temp directory may be on another.
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        // Without this, a power cut can leave a renamed-but-empty file.
        file.sync_all()?;
    }

    match rename_patiently(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}

/// Renames, giving Windows a moment when something briefly holds the target.
///
/// On Windows a file that another program has open without sharing deletion
/// cannot be replaced. The usual culprit is not another part of Safe Invest —
/// the standard library opens files with full sharing — but the antivirus or
/// the search indexer, which look at a file for a few milliseconds right after
/// it is written. Failing the save for that would lose a trade to a scan; a
/// short wait is all it takes.
fn rename_patiently(from: &Path, to: &Path) -> io::Result<()> {
    let mut delay = Duration::from_millis(10);
    let mut attempt = 1;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if attempt < RENAME_ATTEMPTS && is_transient(&error) => {
                std::thread::sleep(delay);
                delay = delay.saturating_mul(2);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Whether a failed rename is worth trying again.
#[cfg(windows)]
fn is_transient(error: &io::Error) -> bool {
    // ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION.
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// Elsewhere a rename refused is refused for good.
#[cfg(not(windows))]
fn is_transient(_error: &io::Error) -> bool {
    false
}

/// Removes temporary files a crash left behind.
///
/// A process killed between writing its temporary file and renaming it leaves
/// the file there for good. They are small, but nothing else would ever clean
/// them up. Only files older than `age` go, so a write in progress in the
/// other process is never touched.
pub(crate) fn remove_stale_temp_files(dir: &Path, age: Duration) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        // Exactly the names `write_atomic` gives: `.<uuid>.tmp`, lower case.
        if !(name.starts_with('.') && name.to_ascii_lowercase().ends_with(".tmp")) {
            continue;
        }
        let old_enough = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|elapsed| elapsed >= age);
        if old_enough {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// An exclusive OS lock held for the life of the guard.
///
/// A lock *file* rather than a named mutex: this works the same on Windows and
/// on the Linux CI runner, and the kernel releases it even if the process is
/// killed mid-write — a stale lock file can never wedge the app.
#[derive(Debug)]
pub(crate) struct LockGuard {
    file: File,
}

impl LockGuard {
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        // `File::lock` has been in std since Rust 1.89 — no crate needed.
        file.lock()?;
        Ok(Self { file })
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_replaces_the_whole_file_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partie.json");

        write_atomic(&path, b"premier contenu, assez long").unwrap();
        write_atomic(&path, b"second").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"second");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn only_old_temporary_files_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join(".abandonne.tmp");
        let game = dir.path().join("partie.json");
        fs::write(&temp, b"x").unwrap();
        fs::write(&game, b"{}").unwrap();

        // A fresh temporary file may be a write in progress elsewhere.
        remove_stale_temp_files(dir.path(), Duration::from_secs(3600));
        assert!(temp.exists());

        remove_stale_temp_files(dir.path(), Duration::ZERO);
        assert!(!temp.exists());
        assert!(game.exists(), "seuls les fichiers temporaires partent");
    }
}
