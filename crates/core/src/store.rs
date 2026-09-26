//! The save files, and the locking that lets two processes share them.
//!
//! The window and the MCP server are separate processes writing the same JSON.
//! Two rules keep that safe: every write goes to a temporary file and is then
//! renamed over the target (a reader never sees a half-written game), and every
//! read-modify-write holds an OS file lock for its whole duration (two writers
//! never interleave).

use crate::fsx::{LockGuard, remove_stale_temp_files, write_atomic};
use crate::model::{GameSession, GameSummary};
use crate::paths::Paths;
use std::fs::{self, File};
use std::io::{self, Read};
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
        // An hour is far longer than any write takes, so a file that old was
        // abandoned by a process that died mid-save.
        let abandoned = std::time::Duration::from_secs(3600);
        remove_stale_temp_files(&paths.games_dir(), abandoned);
        remove_stale_temp_files(paths.root(), abandoned);
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

fn read_session(path: &Path) -> Result<GameSession, StoreError> {
    let mut text = String::new();
    File::open(path)?.read_to_string(&mut text)?;
    serde_json::from_str(&text).map_err(StoreError::Corrupt)
}
