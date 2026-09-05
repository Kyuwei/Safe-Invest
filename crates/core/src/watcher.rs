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
/// thread. Dropping the returned handle stops the thread.
#[derive(Debug)]
pub struct StoreWatcher {
    _inner: notify::RecommendedWatcher,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
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

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_stop = std::sync::Arc::clone(&stop);
        std::thread::Builder::new()
            .name("safeinvest-store-watcher".into())
            .spawn(move || debounce_loop(&rx, &thread_stop, &mut on_change))
            .map_err(notify::Error::io)?;

        Ok(Self {
            _inner: watcher,
            stop,
        })
    }
}

impl Drop for StoreWatcher {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn debounce_loop(
    rx: &Receiver<notify::Result<Event>>,
    stop: &std::sync::atomic::AtomicBool,
    on_change: &mut impl FnMut(),
) {
    let mut pending: Option<Instant> = None;
    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }

        match rx.recv_timeout(DEBOUNCE) {
            Ok(Ok(event)) if is_content_change(&event) => pending = Some(Instant::now()),
            // Anything else — an unrelated file, a watcher hiccup, or simply
            // nothing happening — just falls through to the debounce check.
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
