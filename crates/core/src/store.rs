//! The save files, and the locking that lets two processes share them.
//!
//! The window and the MCP server are separate processes writing the same JSON.
//! Two rules keep that safe: every write goes to a temporary file and is then
//! renamed over the target (a reader never sees a half-written game), and every
//! read-modify-write holds an OS file lock for its whole duration (two writers
//! never interleave).

use crate::model::{GameSession, GameSummary};
use crate::paths::Paths;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("partie introuvable : {0}")]
    NotFound(Uuid),
    #[error("fichier de partie illisible : {0}")]
    Corrupt(#[source] serde_json::Error),
    #[error("erreur disque : {0}")]
    Io(#[from] io::Error),
}

/// Reads and writes games under one data directory.
#[derive(Debug, Clone)]
pub struct GameStore {
    paths: Paths,
}

impl GameStore {
    pub fn new(paths: Paths) -> io::Result<Self> {
        paths.ensure_created()?;
        Ok(Self { paths })
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Every saved game, newest activity first. A file that fails to parse is
    /// skipped and logged rather than taking the whole list down with it.
    pub fn list(&self) -> Vec<GameSummary> {
        let Ok(entries) = fs::read_dir(self.paths.games_dir()) else {
            return Vec::new();
        };

        let mut summaries: Vec<GameSummary> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|e| match read_session(&e.path()) {
                Ok(session) => Some(session.summary()),
                Err(error) => {
                    tracing::warn!(path = %e.path().display(), %error, "partie ignorée");
                    None
                }
            })
            .collect();

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        summaries
    }

    pub fn load(&self, id: Uuid) -> Result<GameSession, StoreError> {
        let path = self.paths.game_file(id);
        if !path.exists() {
            return Err(StoreError::NotFound(id));
        }
        read_session(&path)
    }

    pub fn save(&self, session: &GameSession) -> Result<(), StoreError> {
        let _guard = self.lock()?;
        self.save_locked(session)
    }

    pub fn delete(&self, id: Uuid) -> Result<(), StoreError> {
        let _guard = self.lock()?;
        let path = self.paths.game_file(id);
        if !path.exists() {
            return Err(StoreError::NotFound(id));
        }
        fs::remove_file(path)?;
        if self.current_game() == Some(id) {
            self.write_current_locked(None)?;
        }
        Ok(())
    }

    /// Read, change, write — under one lock for the whole operation.
    ///
    /// This is the primitive every mutation uses. Loading a game, editing it
    /// and saving it as three separate calls would let the other process slip a
    /// trade in between and have it silently overwritten.
    pub fn mutate<T, E>(
        &self,
        id: Uuid,
        change: impl FnOnce(&mut GameSession) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        self.mutate_if(id, |session| change(session).map(|outcome| (outcome, true)))
    }

    /// The same, but the change decides whether the file is written.
    ///
    /// The closure returns its outcome and whether anything actually changed.
    /// The portfolio curve needs this: it is offered a reading every time the
    /// dashboard refreshes but keeps one every quarter of an hour, and
    /// rewriting the save on every refresh to store nothing would be a needless
    /// write a minute, forever.
    pub fn mutate_if<T, E>(
        &self,
        id: Uuid,
        change: impl FnOnce(&mut GameSession) -> Result<(T, bool), E>,
    ) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        let _guard = self.lock().map_err(StoreError::from).map_err(E::from)?;

        let path = self.paths.game_file(id);
        if !path.exists() {
            return Err(E::from(StoreError::NotFound(id)));
        }
        let mut session = read_session(&path).map_err(E::from)?;

        let (outcome, changed) = change(&mut session)?;
        if changed {
            self.save_locked(&session).map_err(E::from)?;
        }
        Ok(outcome)
    }

    /// The game the app reopens on launch, and the one MCP tools act on when
    /// no id is given.
    pub fn current_game(&self) -> Option<Uuid> {
        let raw = fs::read_to_string(self.paths.current_game_file()).ok()?;
        let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
        value
            .get("currentGameId")?
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    pub fn set_current_game(&self, id: Option<Uuid>) -> Result<(), StoreError> {
        let _guard = self.lock()?;
        self.write_current_locked(id)
    }

    fn save_locked(&self, session: &GameSession) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec_pretty(session).map_err(StoreError::Corrupt)?;
        write_atomic(&self.paths.game_file(session.id), &bytes)?;
        Ok(())
    }

    fn write_current_locked(&self, id: Option<Uuid>) -> Result<(), StoreError> {
        let body = serde_json::json!({ "currentGameId": id.map(|v| v.to_string()) });
        let bytes = serde_json::to_vec_pretty(&body).map_err(StoreError::Corrupt)?;
        write_atomic(&self.paths.current_game_file(), &bytes)?;
        Ok(())
    }

    fn lock(&self) -> io::Result<LockGuard> {
        LockGuard::acquire(&self.paths.lock_file())
    }
}

/// An exclusive OS lock held for the life of the guard.
///
/// A lock *file* rather than a named mutex: this works the same on Windows and
/// on the Linux CI runner, and the kernel releases it even if the process is
/// killed mid-write — a stale lock file can never wedge the app.
#[derive(Debug)]
struct LockGuard {
    file: File,
}

impl LockGuard {
    fn acquire(path: &Path) -> io::Result<Self> {
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

fn read_session(path: &Path) -> Result<GameSession, StoreError> {
    let mut text = String::new();
    File::open(path)?.read_to_string(&mut text)?;
    serde_json::from_str(&text).map_err(StoreError::Corrupt)
}

/// Writes `bytes` so that `path` is either the old content or the new one, and
/// never a truncated mix of the two.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
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

    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp);
            Err(error)
        }
    }
}
