//! The diagnostic journal: what the program was doing when it went wrong.
//!
//! Logs that only ever went to a console are logs nobody has when it matters.
//! The window has no console at all on Windows, so until now a person whose
//! app misbehaved had nothing to send but a sentence. This keeps the same
//! `tracing` output in a file beside the saves, capped in size, and hands it
//! back as one exportable text file.
//!
//! Two rules govern what is written:
//!
//! * **It is bounded.** One live file and one previous file, a megabyte each.
//!   A journal that grows without end is a bug of its own.
//! * **It carries no secret.** Every value the program treats as a secret is
//!   registered here and replaced on the way out. An exported journal is meant
//!   to be attached to an email, and an API key or the MCP token travelling in
//!   one would be a leak caused by the very feature meant to help.

use crate::paths::Paths;
use std::io::{self, BufRead as _, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// How large the live file grows before it is rolled over.
pub const MAX_BYTES: u64 = 1_048_576;

/// What a secret is replaced with.
pub const REDACTED: &str = "[secret masqué]";

/// Shorter than this and a "secret" would scrub half the file. A real key or
/// token is far longer; anything this short is not one worth hiding.
const MIN_SECRET_LEN: usize = 8;

/// More registered secrets than a person could plausibly have configured.
const MAX_SECRETS: usize = 32;

pub fn dir(paths: &Paths) -> PathBuf {
    paths.root().join("logs")
}

/// The file being written right now.
pub fn file(paths: &Paths) -> PathBuf {
    dir(paths).join("safe-invest.log")
}

/// The previous file, kept so a crash at startup does not erase the run that
/// explains it.
pub fn previous_file(paths: &Paths) -> PathBuf {
    dir(paths).join("safe-invest.log.1")
}

/* ------------------------------------------------------------- secrets */

fn secrets() -> &'static Mutex<Vec<String>> {
    static SECRETS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    SECRETS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Registers a value that must never appear in the journal.
///
/// Called from wherever a secret becomes known in plaintext — unsealing an API
/// key, minting the MCP token — so that redaction does not depend on every
/// future log call remembering to be careful.
pub fn keep_out(secret: &str) {
    let secret = secret.trim();
    if secret.len() < MIN_SECRET_LEN {
        return;
    }
    if let Ok(mut known) = secrets().lock()
        && known.len() < MAX_SECRETS
        && !known.iter().any(|s| s == secret)
    {
        known.push(secret.to_owned());
    }
}

/// Replaces every registered secret in `text`.
pub fn scrub(text: &str) -> String {
    let Ok(known) = secrets().lock() else {
        return text.to_owned();
    };
    let mut out = text.to_owned();
    for secret in known.iter() {
        if out.contains(secret.as_str()) {
            out = out.replace(secret.as_str(), REDACTED);
        }
    }
    out
}

/* ------------------------------------------------------------- writing */

/// The file the journal is written to, rolled over when it grows too large.
#[derive(Debug)]
struct Journal {
    path: PathBuf,
    previous: PathBuf,
    file: Option<std::fs::File>,
    written: u64,
    max_bytes: u64,
}

impl Journal {
    fn open(path: PathBuf, previous: PathBuf, max_bytes: u64) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            path,
            previous,
            file: Some(file),
            written,
            max_bytes,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        // The window and every MCP process append to this same file. If one of
        // them has already rotated it, the live file on disk is a fresh one,
        // smaller than what this handle wrote — and this handle is still
        // pointing at the file that became the previous one. Follow it to the
        // new file rather than rotating a second time, which would throw the
        // other process's recent lines away.
        let on_disk = std::fs::metadata(&self.path).map_or(0, |meta| meta.len());
        if on_disk < self.written {
            self.reopen()?;
            self.written = on_disk;
            return Ok(());
        }

        // Renaming over the old previous file is the whole rotation: two files,
        // never three, and the live one starts empty. The handle is kept until
        // the rename has worked, so a rotation that fails leaves the journal
        // writing where it was instead of writing nowhere.
        std::fs::rename(&self.path, &self.previous)?;
        self.reopen()?;
        self.written = 0;
        Ok(())
    }

    fn reopen(&mut self) -> io::Result<()> {
        self.file = Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?,
        );
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) {
        if self.written + bytes.len() as u64 > self.max_bytes && self.rotate().is_err() {
            // Writing on is better than losing the line. The counter starts
            // again so the next attempt comes after another full file, not on
            // every line from here on.
            self.written = 0;
        }
        if let Some(file) = self.file.as_mut()
            && file.write_all(bytes).is_ok()
        {
            self.written += bytes.len() as u64;
        }
    }
}

/// A cheap-to-clone handle on the journal, usable as a `tracing` writer.
///
/// Every write is scrubbed and no failure ever propagates: a journal that
/// cannot be written is a lost diagnostic, not a reason for the program to
/// stop working.
#[derive(Debug, Clone)]
pub struct Handle {
    journal: Arc<Mutex<Journal>>,
    /// Put in front of every line: which process wrote it.
    tag: Arc<str>,
}

impl Handle {
    /// Opens the journal under `paths`, creating the directory if needed.
    pub fn open(paths: &Paths) -> io::Result<Self> {
        Self::open_as(paths, "")
    }

    /// The same, with every line marked as written by `role` in this process.
    ///
    /// The window and each MCP server write to the one journal, and a line
    /// that does not say which of them wrote it cannot be read against the
    /// others: a refused order in the window and a trade by an AI look alike.
    pub fn open_as(paths: &Paths, role: &str) -> io::Result<Self> {
        let tag = if role.is_empty() {
            String::new()
        } else {
            format!("[{role} {}] ", std::process::id())
        };
        Self::build(paths, MAX_BYTES, tag)
    }

    pub fn with_limit(paths: &Paths, max_bytes: u64) -> io::Result<Self> {
        Self::build(paths, max_bytes, String::new())
    }

    fn build(paths: &Paths, max_bytes: u64, tag: String) -> io::Result<Self> {
        let journal = Journal::open(file(paths), previous_file(paths), max_bytes)?;
        Ok(Self {
            journal: Arc::new(Mutex::new(journal)),
            tag: tag.into(),
        })
    }

    /// `text` with the tag in front of each of its lines.
    fn tagged(&self, text: &str) -> String {
        if self.tag.is_empty() {
            return text.to_owned();
        }
        let mut out = String::with_capacity(text.len() + self.tag.len());
        for line in text.split_inclusive('\n') {
            out.push_str(&self.tag);
            out.push_str(line);
        }
        out
    }
}

impl Write for Handle {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let scrubbed = self.tagged(&scrub(&text));
        if let Ok(mut journal) = self.journal.lock() {
            journal.append(scrubbed.as_bytes());
        }
        // The caller's buffer was consumed whatever the scrubbed length is.
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Ok(mut journal) = self.journal.lock()
            && let Some(file) = journal.file.as_mut()
        {
            let _ = file.flush();
        }
        Ok(())
    }
}

/* ------------------------------------------------------------- reading */

/// The last `limit` lines, previous file included, oldest first.
///
/// A missing journal is an empty one: on a first run there is nothing to read
/// and that is not an error to report to anybody.
pub fn tail(paths: &Paths, limit: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for path in [previous_file(paths), file(paths)] {
        read_lines(&path, &mut lines);
    }
    if lines.len() > limit {
        lines.drain(..lines.len() - limit);
    }
    lines
}

fn read_lines(path: &Path, into: &mut Vec<String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    // Reading stops at the first line that will not decode. The journal is
    // written as UTF-8 by this program, so that only happens at a tail cut
    // short by a crash or a full disk — and everything before it is still good.
    into.extend(io::BufReader::new(file).lines().map_while(Result::ok));
}

/// How much the journal currently occupies, both files together.
pub fn size(paths: &Paths) -> u64 {
    [previous_file(paths), file(paths)]
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum()
}

/// Writes one file holding `header` and the whole journal, and returns where.
///
/// The name carries the moment it was taken, so two exports never overwrite
/// each other and whoever receives one can tell which run it describes.
pub fn export(paths: &Paths, into: &Path, header: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(into)?;
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let destination = into.join(format!("safe-invest-journal-{stamp}.txt"));

    let mut out = std::fs::File::create(&destination)?;
    writeln!(out, "{}", scrub(header))?;
    writeln!(out)?;

    let mut empty = true;
    for path in [previous_file(paths), file(paths)] {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if !text.trim().is_empty() {
                empty = false;
            }
            out.write_all(scrub(&text).as_bytes())?;
        }
    }
    if empty {
        writeln!(out, "(journal vide)")?;
    }
    out.flush()?;
    Ok(destination)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    fn paths_in(dir: &tempfile::TempDir) -> Paths {
        Paths::at(dir.path())
    }

    #[test]
    fn lines_written_come_back_out() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut handle = Handle::open(&paths).unwrap();

        handle
            .write_all(b"premiere ligne\ndeuxieme ligne\n")
            .unwrap();
        handle.flush().unwrap();

        assert_eq!(tail(&paths, 10), ["premiere ligne", "deuxieme ligne"]);
    }

    #[test]
    fn the_journal_rolls_over_instead_of_growing_without_end() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut handle = Handle::with_limit(&paths, 200).unwrap();

        for i in 0..40 {
            handle
                .write_all(format!("ligne numero {i}\n").as_bytes())
                .unwrap();
        }
        handle.flush().unwrap();

        assert!(
            previous_file(&paths).exists(),
            "aucun fichier précédent : rien n'a tourné"
        );
        assert!(
            std::fs::metadata(file(&paths)).unwrap().len() <= 200,
            "le fichier courant a dépassé sa limite"
        );

        // Rotation must not lose the recent past: the newest line is still there.
        let lines = tail(&paths, 100);
        assert!(lines.iter().any(|line| line == "ligne numero 39"));
    }

    /// The bug this guards: the handle was dropped before the rename, so a
    /// rotation that failed — a file held open elsewhere on Windows — left the
    /// journal writing nowhere for the rest of the run.
    #[test]
    fn a_rotation_that_fails_does_not_silence_the_journal() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        // A directory where the previous file should go makes the rename fail.
        std::fs::create_dir_all(previous_file(&paths)).unwrap();
        let mut handle = Handle::with_limit(&paths, 100).unwrap();

        for i in 0..20 {
            handle.write_all(format!("ligne {i}\n").as_bytes()).unwrap();
        }
        handle.flush().unwrap();

        let text = std::fs::read_to_string(file(&paths)).unwrap();
        assert!(text.contains("ligne 19"), "{text}");
    }

    /// Two processes share the file. When one rotates, the other follows it
    /// to the new file instead of rotating again over the first one's lines.
    #[test]
    fn a_second_writer_follows_a_rotation_made_by_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut window = Handle::with_limit(&paths, 120).unwrap();
        let mut server = Handle::with_limit(&paths, 120).unwrap();

        for i in 0..40 {
            window
                .write_all(format!("fenetre {i}\n").as_bytes())
                .unwrap();
            server
                .write_all(format!("serveur {i}\n").as_bytes())
                .unwrap();
        }

        let lines = tail(&paths, 1000);
        assert!(lines.iter().any(|l| l == "fenetre 39"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "serveur 39"), "{lines:?}");
    }

    #[test]
    fn each_line_says_which_process_wrote_it() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut handle = Handle::open_as(&paths, "mcp").unwrap();

        handle.write_all(b"premiere\nseconde\n").unwrap();
        handle.flush().unwrap();

        let tag = format!("[mcp {}] ", std::process::id());
        assert_eq!(
            tail(&paths, 10),
            [format!("{tag}premiere"), format!("{tag}seconde")]
        );
    }

    #[test]
    fn the_tail_keeps_the_last_lines_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut handle = Handle::open(&paths).unwrap();

        for i in 0..10 {
            handle.write_all(format!("l{i}\n").as_bytes()).unwrap();
        }
        handle.flush().unwrap();

        assert_eq!(tail(&paths, 3), ["l7", "l8", "l9"]);
    }

    /// The reason this module exists at all is that the file gets sent to
    /// somebody. A key in it would be a leak caused by the help feature.
    #[test]
    fn a_registered_secret_never_reaches_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut handle = Handle::open(&paths).unwrap();

        keep_out("cle-tres-secrete-0123456789");
        handle
            .write_all(b"appel a l'API avec cle-tres-secrete-0123456789 en parametre\n")
            .unwrap();
        handle.flush().unwrap();

        let text = std::fs::read_to_string(file(&paths)).unwrap();
        assert!(!text.contains("cle-tres-secrete-0123456789"), "{text}");
        assert!(text.contains(REDACTED), "{text}");
    }

    #[test]
    fn a_value_too_short_to_be_a_secret_is_not_registered() {
        keep_out("abc");
        assert_eq!(scrub("abc def"), "abc def");
    }

    #[test]
    fn the_export_carries_the_header_and_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(&dir);
        let mut handle = Handle::with_limit(&paths, 60).unwrap();

        for i in 0..20 {
            handle
                .write_all(format!("evenement {i}\n").as_bytes())
                .unwrap();
        }
        handle.flush().unwrap();

        let out = tempfile::tempdir().unwrap();
        let exported = export(&paths, out.path(), "Safe Invest 0.0.0-test").unwrap();

        let text = std::fs::read_to_string(&exported).unwrap();
        assert!(text.contains("Safe Invest 0.0.0-test"));
        assert!(text.contains("evenement 19"));
        assert!(
            exported
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("safe-invest-journal-"),
            "{exported:?}"
        );
    }

    /// A first run has nothing to show and must say so rather than fail.
    #[test]
    fn exporting_an_empty_journal_still_produces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let exported = export(&paths_in(&dir), out.path(), "entête").unwrap();

        assert!(
            std::fs::read_to_string(exported)
                .unwrap()
                .contains("(journal vide)")
        );
    }

    #[test]
    fn a_secret_in_the_header_is_scrubbed_too() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        keep_out("jeton-de-la-partie-999999");

        let exported = export(
            &paths_in(&dir),
            out.path(),
            "jeton : jeton-de-la-partie-999999",
        )
        .unwrap();

        let text = std::fs::read_to_string(exported).unwrap();
        assert!(!text.contains("jeton-de-la-partie-999999"), "{text}");
    }
}
