//! E3 — the rotating file log, and the only way to read it back.
//!
//! ## The gap this closes
//!
//! `tracing_subscriber::fmt()` wrote to **stdout and nowhere else**. For a
//! developer with a terminal that is fine; for the people who actually run this
//! app it is nothing at all — a macOS `.app` launched from Finder discards
//! stdout entirely, and a shipped Windows build has no console attached. So
//! everything the backend knew about a Sunday morning went to a file descriptor
//! pointed at nothing.
//!
//! ## Why not `tracing-appender`
//!
//! `tracing-appender` is small, pure Rust and would be entirely proportionate —
//! except that its `rolling::Rotation` is `MINUTELY | HOURLY | DAILY | NEVER`.
//! It has **no size-based rotation**, and size is the axis that matters here: a
//! 90-minute service produces a burst of log and an idle weekday produces almost
//! none, so a daily file is either mostly empty or blows past any cap in one
//! afternoon. Its `NonBlocking` writer is also lossy-or-blocking by
//! configuration, and blocking is not an option on a machine driving a
//! projector. What is left of it after removing time rotation and back-pressure
//! is a thread and a channel — which is what this module is, and it is the same
//! decision SundayRec's E2 reached and has since run a fleet on.
//!
//! ## Never block the producer
//!
//! The thread that logged does the minimum: it formats into a buffer and hands
//! the bytes to a BOUNDED channel with `try_send`. It never touches the disk,
//! never scrubs (that is the writer thread's job) and takes no lock a slow disk
//! could be holding. A full channel DROPS the line and counts it
//! ([`dropped_lines`]) rather than waiting. A log line that costs a cue is not
//! worth having.
//!
//! ## Two-phase start
//!
//! The subscriber is installed at the very top of `run()`, before the Tauri
//! builder, so a panic during plugin setup is already covered — but the app-data
//! directory is only known inside `setup`. So [`writer`] hands back the producer
//! side immediately (lines queue up) and [`start`] spawns the writer thread once
//! the directory exists. The queue absorbs the gap; on any real machine it is a
//! handful of lines.
//!
//! ## Scrubbed twice
//!
//! Once on the WRITER thread, before bytes reach the file, and once again in
//! [`tail`] when they are read back. Belt and braces on purpose: the log tail is
//! the single most dangerous free-text field in the whole programme (it is what
//! E6's "report a problem" dialog attaches), and the two passes fail
//! independently.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};

use crate::telemetry::scrub;

/// Base name of the live log file. Rotated siblings are `sundaystage.1.log` …
/// `sundaystage.4.log`, so the NEWEST is always the one without a number.
const LOG_STEM: &str = "sundaystage";

/// Directory under the app-local data dir. No path ever comes from the
/// frontend — see [`tail`].
const LOG_DIR_NAME: &str = "logs";

/// Rotate once a file would exceed this. 2 MB is roughly a very chatty
/// three-hour session at `info`, so a single service always fits in one file.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// How many files are kept in total (the live one + 4 rotated) — a 10 MB
/// ceiling. Enough that last Sunday survives a busy week of testing, small
/// enough that nobody has to think about it.
pub const MAX_FILES: usize = 5;

/// Bounded queue between the logging threads and the writer.
const QUEUE_CAPACITY: usize = 1024;

/// Hard cap on what [`tail`] hands back, so a "copy the last log" affordance
/// cannot try to move ten megabytes across the IPC boundary.
pub const TAIL_MAX_BYTES: u64 = 256 * 1024;

/// Hard cap on the LINE count [`tail`] accepts, for the same reason.
pub const TAIL_MAX_LINES: usize = 2_000;

/// Where the log files live. Set by [`start`].
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Lines dropped because the queue was full — i.e. how much of the log is
/// missing. Zero on every machine that is not pathologically slow; non-zero is
/// itself a finding.
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// The receiver, parked between [`writer`] and [`start`].
static PENDING_RX: Mutex<Option<Receiver<Vec<u8>>>> = Mutex::new(None);

/// Phase one: build the producer side and park the receiver.
///
/// Returns the `MakeWriter` for a `tracing_subscriber` layer. Safe to call
/// before any directory is known — lines simply queue.
pub fn writer() -> FileLogWriter {
    let (tx, rx) = sync_channel::<Vec<u8>>(QUEUE_CAPACITY);
    if let Ok(mut parked) = PENDING_RX.lock() {
        *parked = Some(rx);
    }
    FileLogWriter { tx: Arc::new(tx) }
}

/// Phase two: create `<app-data>/logs` and spawn the writer thread.
///
/// Returns `false` when the directory cannot be created or [`writer`] was never
/// called — in which case the app logs to stdout exactly as it did before,
/// which is a degradation, not a failure.
pub fn start(data_dir: &Path) -> bool {
    let dir = data_dir.join(LOG_DIR_NAME);
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let Some(rx) = PENDING_RX.lock().ok().and_then(|mut p| p.take()) else {
        return false;
    };
    let _ = LOG_DIR.set(dir.clone());
    std::thread::Builder::new()
        .name("sundaystage-logfile".into())
        .spawn(move || writer_thread(dir, rx))
        .is_ok()
}

/// The directory the log files live in, if the file log started.
pub fn dir() -> Option<PathBuf> {
    LOG_DIR.get().cloned()
}

/// The live log file, if the file log started.
pub fn current_path() -> Option<PathBuf> {
    dir().map(|d| d.join(format!("{LOG_STEM}.log")))
}

/// How many log lines were dropped because the writer could not keep up.
pub fn dropped_lines() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

// ─────────────────────────────────────────────────────────────────────────────
//   The producer side (runs on whatever thread logged)
// ─────────────────────────────────────────────────────────────────────────────

/// The `MakeWriter` handed to the `tracing_subscriber` file layer.
#[derive(Clone)]
pub struct FileLogWriter {
    tx: Arc<SyncSender<Vec<u8>>>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileLogWriter {
    type Writer = LogSink;
    fn make_writer(&'a self) -> Self::Writer {
        LogSink {
            tx: Arc::clone(&self.tx),
            buf: Vec::with_capacity(256),
        }
    }
}

/// One event's worth of bytes.
///
/// The formatter may call `write` several times per event, so the bytes are
/// accumulated here and handed to the channel ONCE, on drop — otherwise a path
/// could be split across two channel messages and slip past the scrubbing the
/// writer thread applies per message.
pub struct LogSink {
    tx: Arc<SyncSender<Vec<u8>>>,
    buf: Vec<u8>,
}

impl Write for LogSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LogSink {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        // `try_send`, never `send`: this runs on the thread that logged, which
        // may be a live command handler. A full queue means the disk is slower
        // than the log; the line is lost and counted, and the cue goes out.
        if self.tx.try_send(std::mem::take(&mut self.buf)).is_err() {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The writer thread
// ─────────────────────────────────────────────────────────────────────────────

fn writer_thread(dir: PathBuf, rx: Receiver<Vec<u8>>) {
    let mut writer = RotatingWriter::open(dir, MAX_FILE_BYTES, MAX_FILES);
    while let Ok(chunk) = rx.recv() {
        // Scrubbing happens HERE, not in the producer: the cost belongs to the
        // thread whose job is writing, not to whoever happened to log.
        let text = String::from_utf8_lossy(&chunk);
        writer.write_line(scrub::for_log(&text).as_bytes());
    }
}

/// A size-rotating append writer. Not shared: exactly one thread owns it.
struct RotatingWriter {
    dir: PathBuf,
    max_bytes: u64,
    max_files: usize,
    file: Option<std::fs::File>,
    size: u64,
}

impl RotatingWriter {
    fn open(dir: PathBuf, max_bytes: u64, max_files: usize) -> Self {
        let mut w = Self {
            dir,
            max_bytes,
            max_files,
            file: None,
            size: 0,
        };
        w.reopen();
        w
    }

    fn live_path(&self) -> PathBuf {
        self.dir.join(format!("{LOG_STEM}.log"))
    }

    /// Open (or create) the live file in append mode and learn its current
    /// size, so a restart continues the existing file instead of resetting the
    /// rotation budget every launch.
    fn reopen(&mut self) {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.live_path())
        {
            Ok(mut f) => {
                self.size = f.seek(SeekFrom::End(0)).unwrap_or(0);
                self.file = Some(f);
            }
            Err(_) => {
                self.file = None;
                self.size = 0;
            }
        }
    }

    fn write_line(&mut self, bytes: &[u8]) {
        if should_rotate(self.size, bytes.len() as u64, self.max_bytes) {
            self.rotate();
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if file.write_all(bytes).is_ok() {
            self.size += bytes.len() as u64;
        } else {
            // A vanished directory (an ejected volume, a user tidying up) — try
            // once to get it back, then give up until the next line.
            self.file = None;
            let _ = std::fs::create_dir_all(&self.dir);
            self.reopen();
        }
    }

    /// `sundaystage.log` → `sundaystage.1.log` → … , dropping the oldest.
    fn rotate(&mut self) {
        // Close before renaming: Windows will not rename an open file.
        self.file = None;
        let numbered = |n: usize| self.dir.join(format!("{LOG_STEM}.{n}.log"));
        let _ = std::fs::remove_file(numbered(self.max_files - 1));
        // Shift the rest down, oldest first, so nothing overwrites a file that
        // has not moved yet.
        for n in (1..self.max_files - 1).rev() {
            let _ = std::fs::rename(numbered(n), numbered(n + 1));
        }
        let _ = std::fs::rename(self.live_path(), numbered(1));
        self.reopen();
    }
}

/// Whether appending `incoming` bytes to a file of `current` bytes should
/// rotate first.
///
/// An EMPTY file never rotates, however big the incoming line is: a single event
/// larger than the cap would otherwise rotate on every write and grind the whole
/// ring away in one burst. Better one oversized file than no history.
fn should_rotate(current: u64, incoming: u64, max_bytes: u64) -> bool {
    current > 0 && current.saturating_add(incoming) > max_bytes
}

// ─────────────────────────────────────────────────────────────────────────────
//   Reading it back
// ─────────────────────────────────────────────────────────────────────────────

/// The last `lines` lines of the live log, scrubbed AGAIN on the way out.
///
/// Takes NO path. The renderer names a line count and nothing else, so no IPC
/// caller can point this at `~/.ssh/id_rsa` — the same rule SundayRec's
/// `logs_tail` follows. `lines` is clamped to [`TAIL_MAX_LINES`] and the read
/// window to [`TAIL_MAX_BYTES`].
pub fn tail(lines: usize) -> std::io::Result<String> {
    let Some(path) = current_path() else {
        return Ok(String::new());
    };
    tail_of(&path, lines)
}

fn tail_of(path: &Path, lines: usize) -> std::io::Result<String> {
    let want_lines = lines.clamp(1, TAIL_MAX_LINES);
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        // No log file yet is not an error — it is "nothing has been logged".
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e),
    };
    let len = file.metadata()?.len();
    let from = len.saturating_sub(TAIL_MAX_BYTES);
    file.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::with_capacity((len - from) as usize);
    file.read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(trim_to_line_start(&buf, from > 0)).into_owned();
    Ok(scrub::for_log(&last_lines(&text, want_lines)))
}

/// The last `n` non-empty-suffix lines of `text`, in order.
fn last_lines(text: &str, n: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(n);
    all[start..].join("\n")
}

/// Drop a leading partial line (and with it any partial UTF-8 character) when
/// the read started mid-file. A tail that begins halfway through a timestamp is
/// worse than one that begins one line later.
fn trim_to_line_start(buf: &[u8], started_mid_file: bool) -> &[u8] {
    if !started_mid_file {
        return buf;
    }
    match buf.iter().position(|&b| b == b'\n') {
        Some(at) => &buf[at + 1..],
        // No newline at all in the window: one enormous line. Hand back nothing
        // rather than a mangled fragment.
        None => &[],
    }
}

/// The first line of every log: who is running, on what, built how.
///
/// "What build is this?" is the first question every support conversation opens
/// with. Numbers and closed sets only — no paths, per law 2 (the log directory
/// is deliberately NOT logged, unlike SundayRec's banner, because the tail is
/// uploadable and the directory contains the operator's name).
pub fn log_startup_banner() {
    // Computed OUTSIDE the macro on purpose: the audit test below forbids the
    // identifier `path` inside a tracing argument, and that rule is worth more
    // than the convenience of inlining this call.
    let file_log = current_path().is_some();
    tracing::info!(
        version = crate::telemetry::app_version(),
        os = ?crate::telemetry::TelemetryOs::current(),
        arch = std::env::consts::ARCH,
        profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        file_log,
        max_files = MAX_FILES,
        max_bytes = MAX_FILE_BYTES,
        "SundayStage starting"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── rotation arithmetic ──────────────────────────────────────────────────

    #[test]
    fn rotation_triggers_only_when_the_file_would_actually_overflow() {
        let max = 100;
        assert!(!should_rotate(0, 10, max), "an empty file never rotates");
        assert!(!should_rotate(50, 10, max));
        assert!(!should_rotate(90, 10, max), "exactly at the cap still fits");
        assert!(should_rotate(90, 11, max), "one byte over rotates");
        assert!(should_rotate(100, 1, max));
        // The pathological case the empty-file rule exists for: a single event
        // bigger than the whole budget must be written, not rotated forever.
        assert!(!should_rotate(0, max * 10, max));
        assert!(should_rotate(1, max * 10, max));
        // …and the arithmetic cannot wrap.
        assert!(should_rotate(u64::MAX, u64::MAX, max));
    }

    #[test]
    fn writing_past_the_cap_rotates_and_keeps_at_most_max_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = RotatingWriter::open(dir.path().to_path_buf(), 100, MAX_FILES);
        // 40 lines × ~30 bytes over a 100-byte cap = far more rotations than
        // the 5-file ring can hold.
        for i in 0..40 {
            w.write_line(format!("line {i:04} padding padding\n").as_bytes());
        }
        drop(w);
        let logs: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".log"))
            .collect();
        assert_eq!(logs.len(), MAX_FILES, "the ring is capped at MAX_FILES");
        // Every file stays under (cap + one oversized line).
        for entry in &logs {
            let len = entry.metadata().expect("metadata").len();
            assert!(len <= 100 + 30, "{:?} grew to {len}", entry.file_name());
        }
        // The newest content is in the unnumbered live file.
        let live =
            std::fs::read_to_string(dir.path().join(format!("{LOG_STEM}.log"))).expect("live file");
        assert!(live.contains("line 0039"), "{live}");
    }

    #[test]
    fn a_restart_appends_instead_of_resetting_the_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut w = RotatingWriter::open(dir.path().to_path_buf(), 10_000, MAX_FILES);
        w.write_line(b"before restart\n");
        drop(w);
        let mut w = RotatingWriter::open(dir.path().to_path_buf(), 10_000, MAX_FILES);
        assert!(w.size > 0, "the writer learned the existing size");
        w.write_line(b"after restart\n");
        drop(w);
        let body =
            std::fs::read_to_string(dir.path().join(format!("{LOG_STEM}.log"))).expect("live file");
        assert!(body.contains("before restart") && body.contains("after restart"));
    }

    // ── the writer thread scrubs before the bytes land ───────────────────────

    #[test]
    fn the_writer_thread_scrubs_paths_out_of_every_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (tx, rx) = sync_channel::<Vec<u8>>(8);
        let path = dir.path().to_path_buf();
        let handle = std::thread::spawn(move || writer_thread(path, rx));
        tx.send(b"INFO opening /Users/ola/Musikk/salme.mp4\n".to_vec())
            .expect("send");
        drop(tx);
        handle.join().expect("writer thread");
        let body =
            std::fs::read_to_string(dir.path().join(format!("{LOG_STEM}.log"))).expect("live file");
        assert!(!body.contains("/Users/"), "{body}");
        assert!(!body.contains("salme"), "{body}");
        assert!(body.contains("<path>"), "{body}");
    }

    // ── reading it back ──────────────────────────────────────────────────────

    #[test]
    fn the_tail_returns_the_last_n_lines_and_scrubs_again() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("{LOG_STEM}.log"));
        // Write a file that was NOT scrubbed on the way in (an old log from
        // before this module, or a line that slipped past) — the read pass is
        // the second, independent guard.
        let mut body = String::new();
        for i in 0..50 {
            body.push_str(&format!("line {i} at /Users/ola/Musikk/salme-{i}.mp4\n"));
        }
        std::fs::write(&path, body).expect("write");

        let out = tail_of(&path, 5).expect("tail");
        assert_eq!(out.lines().count(), 5);
        assert!(out.contains("line 49"), "{out}");
        assert!(!out.contains("line 44"), "{out}");
        assert!(!out.contains("/Users/"), "{out}");
        assert!(!out.contains("salme"), "{out}");
    }

    #[test]
    fn the_tail_clamps_the_requested_line_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("{LOG_STEM}.log"));
        std::fs::write(&path, "a\nb\nc\n").expect("write");
        // 0 clamps up to 1 (never an empty answer for a non-empty file)…
        assert_eq!(tail_of(&path, 0).expect("tail"), "c");
        // …and an absurd request clamps down to what the file holds.
        assert_eq!(tail_of(&path, usize::MAX).expect("tail").lines().count(), 3);
    }

    #[test]
    fn a_missing_log_is_empty_not_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(tail_of(&dir.path().join("nope.log"), 10).expect("tail"), "");
        // …and with no log directory armed at all, so is the public entry point.
        if current_path().is_none() {
            assert_eq!(tail(10).expect("tail"), "");
        }
    }

    #[test]
    fn a_huge_log_is_read_from_the_tail_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(format!("{LOG_STEM}.log"));
        let mut body = String::from("FIRST-LINE-MARKER\n");
        // Comfortably past TAIL_MAX_BYTES.
        for i in 0..12_000 {
            body.push_str(&format!("padding line {i:06} ------------------------\n"));
        }
        body.push_str("LAST-LINE-MARKER\n");
        std::fs::write(&path, body).expect("write");
        let out = tail_of(&path, TAIL_MAX_LINES).expect("tail");
        assert!(
            out.contains("LAST-LINE-MARKER"),
            "the newest line is present"
        );
        assert!(
            !out.contains("FIRST-LINE-MARKER"),
            "the byte window bounds the read"
        );
        assert!(out.len() as u64 <= TAIL_MAX_BYTES);
    }

    #[test]
    fn a_mid_file_read_drops_the_partial_first_line() {
        assert_eq!(trim_to_line_start(b"half\nfull\n", true), b"full\n");
        assert_eq!(trim_to_line_start(b"half\nfull\n", false), b"half\nfull\n");
        // One enormous line with no newline in the window yields nothing rather
        // than a mangled fragment.
        assert_eq!(trim_to_line_start(b"nonewline", true), b"");
    }

    // ── the tracing audit (law 2 applied to the formatters themselves) ───────

    /// Identifiers that name user content, a credential, or an absolute path.
    ///
    /// The scrubber removes PATHS. It cannot remove a song title, a service
    /// name or a device name, because none of them are path-shaped — so the
    /// only defence for those is the rule that no formatter interpolates one.
    const FORBIDDEN: &[&str] = &[
        "title",
        "lyric",
        "text_lines",
        "notes",
        "display_label",
        "device",
        "path",
        "api_key",
        "apikey",
        "token",
        "secret",
        "password",
        // `{e}`/`{err}` on an error type: `AppError::Json` echoes the offending
        // slide content, and an `io::Error` can carry a filename. Log
        // `e.code()`, a count, or a scrubbed copy instead.
        "message",
    ];

    /// Inline captures that are an ERROR VALUE by name: `"…: {e}"`.
    ///
    /// Matched exactly rather than by substring, because `e` as a substring is
    /// in almost every word. This is the single most common leak shape in the
    /// tree and the one the E3 audit found twice: `AppError::Json` formats the
    /// value serde choked on (for a slide, the lyric), and `io::Error` can name
    /// a file. Log `e.code()`, a count, or `scrub::for_log(&e.to_string())`.
    const FORBIDDEN_CAPTURES: &[&str] = &["e", "err", "error", "msg", "detail", "why"];

    /// Call sites allowed to mention a forbidden identifier, with the reason.
    /// An allowlist entry is a decision on the record, not a silencer.
    const AUDIT_ALLOW: &[(&str, &str, &str)] = &[(
        "telemetry/crash_ring.rs",
        "message",
        "the crash entry's message is scrubbed AND capped by construction \
         before this line can format it",
    )];

    /// Every `tracing::` call site in `src-tauri/src`, pinned.
    ///
    /// The E3 audit found and fixed three real leaks: `{e}` on an `AppError`
    /// whose `Json` variant echoes the offending slide content; `{message}`
    /// from the output child, which is a `serde_json` error formatted over a
    /// Render frame and therefore echoes lyrics; and `{e}` on the supervisor's
    /// `io::Error`, which can name the socket. This test is what stops a
    /// fourth from being written.
    #[test]
    fn no_tracing_call_site_interpolates_content() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        let mut spans_seen = 0usize;
        for file in rust_sources(&root) {
            let body = std::fs::read_to_string(&file).expect("readable source");
            let shown = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .display()
                .to_string();
            // Only SHIPPED call sites. A `#[cfg(test)] mod tests` never runs on
            // an operator's machine, and this module's own fixtures below are
            // deliberately-leaky examples that would otherwise fail the audit
            // they exist to exercise.
            let shipped = &body[..body.find("\n#[cfg(test)]").unwrap_or(body.len())];
            for span in tracing_spans(shipped) {
                spans_seen += 1;
                for needle in audited_identifiers(&span) {
                    let allowed = AUDIT_ALLOW
                        .iter()
                        .any(|(f, n, _)| shown.ends_with(f) && *n == needle);
                    if !allowed {
                        offenders.push(format!("{shown}: `{needle}` in `{span}`"));
                    }
                }
            }
        }
        // A guard on the guard: if the extractor ever stops matching (a moved
        // manifest dir, a renamed `src`), the assertion below would pass
        // vacuously and the audit would quietly stop auditing.
        assert!(
            spans_seen >= 10,
            "the extractor only found {spans_seen} tracing call sites"
        );
        assert!(
            offenders.is_empty(),
            "tracing call sites must log ids and counts, never content:\n{}",
            offenders.join("\n")
        );
    }

    /// The forbidden identifiers a single `tracing::…!(…)` span mentions.
    ///
    /// Two places are checked, because a value reaches a log line two ways:
    ///
    ///   * inside a string literal, as an inline capture (`"… {message}"`);
    ///   * outside one, as a field value or positional argument
    ///     (`title = %song.title`, `"…: {}", song.title`).
    ///
    /// Prose inside a literal is NOT a finding — "companion broadcast failed"
    /// mentions nothing.
    fn audited_identifiers(span: &str) -> Vec<String> {
        let (literals, code) = split_literals(span);
        let mut names: Vec<String> = Vec::new();
        for lit in literals {
            let mut rest = lit.as_str();
            while let Some(open) = rest.find('{') {
                rest = &rest[open + 1..];
                let Some(close) = rest.find('}') else { break };
                // `{{` escapes and `{}` positionals name nothing.
                let name = rest[..close].split(':').next().unwrap_or("").trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
                rest = &rest[close + 1..];
            }
        }
        let mut found: Vec<String> = Vec::new();
        for name in &names {
            if FORBIDDEN_CAPTURES.contains(&name.as_str()) {
                found.push(format!("{{{name}}}"));
            }
        }
        // The same shape written as a FIELD rather than an inline capture:
        // `tracing::error!(detail = %e, "…")`. Each comma-separated fragment's
        // right-hand side is compared exactly, so `code = e.code()` and
        // `reason = %scrub::for_log(&e.to_string())` — the fixed forms — pass
        // while the raw value does not.
        for fragment in code.split(',') {
            let rhs = fragment
                .rsplit('=')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches(['%', '?', '&']);
            if FORBIDDEN_CAPTURES.contains(&rhs) {
                found.push(rhs.to_string());
            }
        }
        let haystack = format!("{} {}", names.join(" "), code);
        for needle in FORBIDDEN {
            if haystack.contains(needle) {
                found.push((*needle).to_string());
            }
        }
        found.sort();
        found.dedup();
        found
    }

    /// Split a span into its string-literal contents and everything else.
    /// Deliberately simple: escapes are honoured, raw strings are not used in
    /// any tracing call in this tree.
    fn split_literals(span: &str) -> (Vec<String>, String) {
        let mut literals = Vec::new();
        let mut code = String::new();
        let mut current = String::new();
        let mut in_lit = false;
        let mut escaped = false;
        for c in span.chars() {
            if in_lit {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_lit = false;
                    literals.push(std::mem::take(&mut current));
                    continue;
                }
                current.push(c);
            } else if c == '"' {
                in_lit = true;
            } else {
                code.push(c);
            }
        }
        if in_lit {
            literals.push(current);
        }
        (literals, code)
    }

    /// Every `tracing::<level>!( … )` invocation in `body`, as source text.
    /// Matches the macro form only, so `use tracing::warn` and a mention inside
    /// a string are both ignored.
    fn tracing_spans(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        let bytes = body.as_bytes();
        let mut from = 0usize;
        while let Some(at) = body[from..].find("tracing::") {
            let start = from + at;
            let after = start + "tracing::".len();
            let level_end = body[after..]
                .find(|c: char| !c.is_ascii_lowercase() && c != '_')
                .map(|n| after + n)
                .unwrap_or(body.len());
            from = after;
            // Must be `tracing::level!(` to be an invocation.
            if bytes.get(level_end) != Some(&b'!') || bytes.get(level_end + 1) != Some(&b'(') {
                continue;
            }
            let mut depth = 0i32;
            let mut end = level_end + 1;
            let mut in_lit = false;
            let mut escaped = false;
            for (i, c) in body[level_end + 1..].char_indices() {
                let abs = level_end + 1 + i;
                if in_lit {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        in_lit = false;
                    }
                    continue;
                }
                match c {
                    '"' => in_lit = true,
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = abs + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            out.push(body[start..end].to_string());
            from = end;
        }
        out
    }

    /// Every `.rs` file under `dir`, recursively.
    fn rust_sources(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in rd.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                out.extend(rust_sources(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
        out.sort();
        out
    }

    #[test]
    fn the_audit_catches_the_leaks_it_was_written_for() {
        // The three shapes E3 actually fixed, plus the two prose cases that
        // must NOT trip it — otherwise the audit is either blind or so noisy
        // nobody keeps it.
        let leaky = [
            r#"tracing::error!("skipping item {}: {e}", item.id);"#,
            r#"tracing::warn!("output {} reported: {message}", spec.label);"#,
            r#"tracing::info!(path = %file.display(), "wrote");"#,
            r#"tracing::warn!("could not open {}", song.title);"#,
            r#"tracing::error!(detail = %e, "publish failed");"#,
        ];
        for src in leaky {
            let spans = tracing_spans(src);
            assert_eq!(spans.len(), 1, "extractor missed: {src}");
            assert!(
                !audited_identifiers(&spans[0]).is_empty(),
                "audit missed a leak: {src}"
            );
        }
        let clean = [
            r#"tracing::warn!(kind = "companion", "companion broadcast failed");"#,
            r#"tracing::error!(item_id = %item.id, code = e.code(), "skipping corrupt item");"#,
            r#"tracing::info!(count = n, "cue list compiled");"#,
        ];
        for src in clean {
            let spans = tracing_spans(src);
            assert_eq!(spans.len(), 1, "extractor missed: {src}");
            assert!(
                audited_identifiers(&spans[0]).is_empty(),
                "false positive on: {src} → {:?}",
                audited_identifiers(&spans[0])
            );
        }
        // A mention that is not an invocation is not a call site.
        assert!(tracing_spans("use tracing::warn;").is_empty());
    }
}
