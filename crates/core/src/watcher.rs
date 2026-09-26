//! Noticing that the other process changed a game.
//!
//! This is what makes AI mode live: the MCP server writes a trade, and the open
//! window redraws within a second without polling the disk.

use crate::paths::Paths;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// Editors and atomic renames emit several events for one logical change;
/// this is how long we wait for the storm to settle.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Calls `on_change` shortly after any game file is written, from a background
/// thread. Dropping the returned handle stops the thread: the watcher owns the
/// sending half of the channel, and the thread ends when it closes.
#[derive(Debug)]
pub struct StoreWatcher {
    _inner: notify::RecommendedWatcher,
}

impl StoreWatcher {
    pub fn start(
        paths: &Paths,
        mut on_change: impl FnMut() + Send + 'static,
    ) -> notify::Result<Self> {
        paths.ensure_created().map_err(notify::Error::io)?;

        let (tx, rx) = channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(move |event| {
            // A full channel means the UI is behind; dropping an event is fine
            // because the next one still triggers a full reload.
            let _ = tx.send(event);
        })?;
        watcher.watch(&paths.games_dir(), RecursiveMode::NonRecursive)?;

        std::thread::Builder::new()
            .name("safeinvest-store-watcher".into())
            .spawn(move || debounce_loop(&rx, &mut on_change))
            .map_err(notify::Error::io)?;

        Ok(Self { _inner: watcher })
    }
}

/// Waits for changes and reports each burst once, when it has settled.
///
/// With nothing pending the thread sleeps on the channel — it used to wake
/// four times a second for the whole life of the window, on a laptop's
/// battery, to find out that nothing had happened.
fn debounce_loop(rx: &Receiver<notify::Result<Event>>, on_change: &mut impl FnMut()) {
    let mut pending: Option<Instant> = None;
    loop {
        let received = match pending {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => rx.recv_timeout(DEBOUNCE.saturating_sub(at.elapsed())),
        };

        match received {
            Ok(Ok(event)) if is_content_change(&event) => pending = Some(Instant::now()),
            // Anything else — an unrelated file, a watcher hiccup, or the
            // burst going quiet — falls through to the debounce check.
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }

        if pending.is_some_and(|at| at.elapsed() >= DEBOUNCE) {
            pending = None;
            on_change();
        }
    }
}

fn is_content_change(event: &Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    // Ignore our own temporary files, or the watcher fires twice per save.
    event.paths.iter().any(|p| is_game_file(p))
}

fn is_game_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "json")
        && !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The whole point of the watcher: a trade written by the other process
    /// reaches the window without it polling anything.
    #[test]
    fn a_game_written_elsewhere_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        let (tx, rx) = channel();
        let watcher = StoreWatcher::start(&paths, move || {
            let _ = tx.send(());
        })
        .unwrap();

        for round in 0..3 {
            std::fs::write(paths.games_dir().join("partie.json"), format!("{round}")).unwrap();
        }

        rx.recv_timeout(Duration::from_secs(10))
            .expect("aucune notification après l'écriture d'une partie");
        drop(watcher);
    }

    #[test]
    fn temporary_files_are_not_games() {
        assert!(is_game_file(Path::new("/x/games/abc.json")));
        assert!(!is_game_file(Path::new("/x/games/.abc.tmp")));
        assert!(!is_game_file(Path::new("/x/games/.abc.json")));
        assert!(!is_game_file(Path::new("/x/games/notes.txt")));
    }
}
