//! Where Safe Invest keeps its files.
//!
//! One directory, shared by the window and the MCP server — that shared
//! directory *is* the integration between the two.

use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Point the whole app at another directory. Used by the tests and by
/// `--data-dir`, so a demo run never touches a real save.
pub const DATA_DIR_ENV: &str = "SAFEINVEST_DATA_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// `%LOCALAPPDATA%\SafeInvest` on Windows, `~/.local/share/SafeInvest`
    /// elsewhere, unless [`DATA_DIR_ENV`] overrides it.
    pub fn discover() -> Self {
        if let Some(dir) = std::env::var_os(DATA_DIR_ENV)
            && !dir.is_empty()
        {
            return Self::at(PathBuf::from(dir));
        }

        let root = directories::ProjectDirs::from("", "", "SafeInvest").map_or_else(
            || PathBuf::from(".safeinvest"),
            |dirs| dirs.data_local_dir().to_path_buf(),
        );
        Self::at(root)
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn games_dir(&self) -> PathBuf {
        self.root.join("games")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn current_game_file(&self) -> PathBuf {
        self.root.join("current.json")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.root.join(".store.lock")
    }

    pub fn game_file(&self, id: Uuid) -> PathBuf {
        self.games_dir().join(format!("{}.json", id.simple()))
    }

    pub fn ensure_created(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.games_dir())?;
        restrict_to_owner(&self.root)
    }
}

/// On Unix the data directory holds API keys that are not DPAPI-protected, so
/// it is made owner-only. On Windows `%LOCALAPPDATA%` is already per-user and
/// the keys are encrypted on top of that.
#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
