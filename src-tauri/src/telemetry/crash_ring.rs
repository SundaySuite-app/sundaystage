//! E3 — the structured crash ring.
//!
//! ## What this replaces
//!
//! `services::crash` wrote a plain-text file per panic, unbounded and
//! unscrubbed:
//!
//! ```text
//! SundayStage crash report
//! version: 0.5.0
//! when: 1754640000000 (unix ms)
//! where: /Users/ola/dev/sundaystage/src-tauri/src/commands/live.rs:288:9
//! what: called `Option::unwrap()` on a `None` value
//! ```
//!
//! Three problems, all fatal for a file that E5 will offer to upload: the
//! operator's name was in every `where:` line on a machine built from source,
//! nothing capped the directory (a crash loop filled the disk), and no machine
//! could read it back — which is what a payload builder needs to do.
//!
//! ## The shape
//!
//! One JSON [`CrashEntry`] per record, in a ring capped at [`RING_MAX`],
//! newest-wins, under `<app-data>/crashes/`. Ordering comes from the FILENAME
//! (zero-padded unix ms + a sequence number), not from any shared state, so the
//! ring needs no lock and a plain lexicographic sort is chronological order.
//!
//! ## Constraints the panic hook must respect
//!
//! It runs on an ARBITRARY thread, possibly while unwinding, possibly while
//! tokio is already coming down. So:
//!
//!   - **No tokio, no app handle, no repository.** The directory is resolved
//!     ONCE at startup into a [`OnceLock`] and the write is plain `std::fs`.
//!   - **No lock this process holds elsewhere.** Nothing here takes a mutex any
//!     other module can be inside — in particular not `AppState.live`.
//!   - **It must not panic.** The whole body runs inside `catch_unwind`; a panic
//!     inside a panic hook ABORTS the process, turning a recoverable command
//!     failure into a hard kill mid-service.
//!   - **It must not block.** One small write, no retry, no fsync, no directory
//!     scan larger than the ring.
//!
//! This is law 1's single exception, and it is bounded: at panic time there is
//! no live path left in THIS process to protect, and the output child survives
//! as its own process holding the last frame.
//!
//! ## Opt-in
//!
//! Local capture stays gated on the existing `crash_reporting.json` flag, off
//! by default, exactly as it was. That flag is about writing files on this
//! machine; NETWORK consent is a separate, later decision (E5) and this flag
//! must never be read as granting it.

use std::panic::{AssertUnwindSafe, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::telemetry::scrub;
use crate::telemetry::{
    app_version, now_ms, TelemetryOs, LOCATION_MAX_CHARS, MESSAGE_MAX_CHARS, RECORD_SCHEMA,
    TASK_MAX_CHARS,
};

/// How many crash records are kept, newest first.
///
/// Twenty is chosen against the question these files exist to answer: "does
/// this machine crash, and does it crash the same way every time?" One record
/// answers neither; a machine crashing on every launch tells its whole story
/// inside twenty. It also matches the payload cap E4 pins server-side, so one
/// upload can carry everything the ring holds and nothing is silently dropped.
pub const RING_MAX: usize = 20;

/// Filename prefix for a record. One prefix, one ring: unlike SundayRec there
/// is no second "restart" ring here, because an output-child restart is NOT a
/// crash — it is a quality signal (see [`crate::telemetry::quality`]).
const PREFIX: &str = "crash-";

/// The opt-in flag file, unchanged from `services::crash` so an existing
/// install keeps its choice across the upgrade.
const FLAG_FILE: &str = "crash_reporting.json";

/// Directory name under the app-data dir, unchanged for the same reason.
const CRASH_DIR: &str = "crashes";

/// The directory records are written to. Resolved ONCE at startup (see
/// [`arm`]) so the panic hook never touches a path resolver or an app handle.
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Disambiguates two records written inside the same millisecond.
static SEQ: AtomicU32 = AtomicU32::new(0);

/// What kind of crash-adjacent event a record describes. A CLOSED set: this is
/// the field an aggregate is grouped by, and a free-form string here would be
/// the one place a caller could widen the wire.
///
/// An output-child death is deliberately absent — that is a quality signal, not
/// a crash: the projector kept showing its frame throughout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CrashKind.ts")]
#[serde(rename_all = "snake_case")]
pub enum CrashKind {
    /// The process panic hook fired.
    Panic,
    /// A watched background task ended by panicking.
    TaskPanic,
    /// The renderer's `window.onerror` fired.
    WebviewError,
    /// The renderer's `unhandledrejection` fired.
    UnhandledRejection,
    /// Anything a later release adds that this one cannot name.
    Other,
}

/// One persisted crash record — already wire-shaped.
///
/// The fields are exactly the programme's `crashes` shape. Everything free-form
/// has been through [`scrub`] and is capped, so the file on disk is what a
/// later release would send, byte for byte. There is no second truncation pass
/// that could disagree with the Worker's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CrashEntry.ts")]
#[serde(rename_all = "camelCase")]
pub struct CrashEntry {
    /// [`RECORD_SCHEMA`] at write time.
    pub schema: u32,
    pub kind: CrashKind,
    /// Unix ms (UTC) the record was written.
    #[ts(type = "number")]
    pub at: i64,
    /// The app version that crashed — often NOT the version reading the ring,
    /// which is exactly why it travels with the record.
    pub app_version: String,
    pub os: TelemetryOs,
    /// Scrubbed, capped at [`MESSAGE_MAX_CHARS`] (+1 for the ellipsis).
    pub message: String,
    /// `file:line:col`, same treatment. `None` when the source carried none.
    pub location: Option<String>,
    /// The task / component name, for the task kinds.
    pub task: Option<String>,
    /// Whether a backtrace exists ON THIS MACHINE. The backtrace itself is not
    /// stored in v1: it names every source file it walked through, which on a
    /// machine built from source is a directory listing of someone's disk. The
    /// boolean answers the only question an aggregate can ask of it ("was one
    /// available?") at zero privacy cost.
    pub backtrace_present: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
//   Arming
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve the crash directory. Call ONCE from `setup`, before anything can
/// panic in earnest.
///
/// Separate from [`install_panic_hook`] because the hook is installed as early
/// as possible (before the Tauri builder, so a panic during plugin setup is
/// covered) while the directory is only known once Tauri's path resolver
/// exists. A panic in that window logs but writes no file — the honest
/// degradation, and the alternative (mirroring Tauri's path computation by
/// hand) is a second source of truth that can silently drift.
pub fn arm(data_dir: &Path) {
    let _ = DIR.set(data_dir.join(CRASH_DIR));
}

/// The crash directory, if one was resolved.
pub fn dir() -> Option<PathBuf> {
    DIR.get().cloned()
}

/// Install the panic hook. Call as early in `run()` as possible.
///
/// The previous hook is CHAINED, not replaced, so stderr behaviour in a dev
/// terminal is unchanged: this ADDS a file, it does not take away the message
/// you are used to reading.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic inside a panic hook ABORTS. Everything below is best-effort
        // forensics; none of it is worth turning a recoverable failure into a
        // hard process kill for.
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| persist_panic(info)));
        previous(info);
    }));
}

/// Format + persist a panic. Best-effort throughout.
fn persist_panic(info: &PanicHookInfo<'_>) {
    let entry = build_panic_entry(
        info,
        scrub::home_dir().as_deref(),
        now_ms(),
        std::backtrace::Backtrace::force_capture().status()
            != std::backtrace::BacktraceStatus::Disabled,
    );
    // The log line first: on a dev box with a terminal this is the fastest
    // signal, and it costs nothing if the file write below fails.
    tracing::error!(
        kind = ?entry.kind,
        location = ?entry.location,
        "PANIC: {}",
        entry.message
    );
    write_if_enabled(&entry);
}

/// Build the record for a panic. Pure given its inputs (the hook supplies the
/// home dir, the clock and the backtrace status), so the formatting, truncation
/// and scrubbing are all unit-testable without panicking a test process.
fn build_panic_entry(
    info: &PanicHookInfo<'_>,
    home: Option<&str>,
    at: i64,
    backtrace_present: bool,
) -> CrashEntry {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string());
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));

    CrashEntry {
        schema: RECORD_SCHEMA,
        kind: CrashKind::Panic,
        at,
        app_version: app_version().to_string(),
        os: TelemetryOs::current(),
        message: scrub::free_text(&message, home, MESSAGE_MAX_CHARS),
        location: location.map(|l| scrub::free_text(&l, home, LOCATION_MAX_CHARS)),
        task: None,
        backtrace_present,
    }
}

/// Build a record for anything that is not the process panic hook: a watched
/// task, or an error the renderer reported over IPC.
///
/// Pure, and the ONLY constructor the IPC command uses — so a renderer cannot
/// reach the ring without going through the same scrubbing and the same caps
/// the panic hook does.
pub fn build_entry(
    kind: CrashKind,
    message: &str,
    location: Option<&str>,
    task: Option<&str>,
    at: i64,
    home: Option<&str>,
) -> CrashEntry {
    CrashEntry {
        schema: RECORD_SCHEMA,
        kind,
        at,
        app_version: app_version().to_string(),
        os: TelemetryOs::current(),
        message: scrub::free_text(message, home, MESSAGE_MAX_CHARS),
        location: location
            .map(|l| scrub::free_text(l, home, LOCATION_MAX_CHARS))
            .filter(|l| !l.is_empty()),
        task: task
            .map(|t| scrub::free_text(t, home, TASK_MAX_CHARS))
            .filter(|t| !t.is_empty()),
        backtrace_present: false,
    }
}

/// Record a crash-adjacent event into the ARMED directory, if the operator
/// opted in. Used by the panic hook and by watched tasks, neither of which has
/// an app handle to ask.
pub fn record(kind: CrashKind, message: &str, location: Option<&str>, task: Option<&str>) {
    let Some(dir) = DIR.get() else { return };
    // The flag lives beside the crash directory, i.e. in the app-data dir.
    let Some(data_dir) = dir.parent().map(Path::to_path_buf) else {
        return;
    };
    record_in(&data_dir, kind, message, location, task);
}

/// Record a crash-adjacent event into an EXPLICIT app-data directory.
///
/// The route for anything that already holds `AppState` — a command has the
/// directory in hand and should not have to trust a process-global to have been
/// armed. Same opt-in gate, same scrubbing, same caps.
pub fn record_in(
    data_dir: &Path,
    kind: CrashKind,
    message: &str,
    location: Option<&str>,
    task: Option<&str>,
) {
    if !is_enabled(data_dir) {
        return;
    }
    let entry = build_entry(
        kind,
        message,
        location,
        task,
        now_ms(),
        scrub::home_dir().as_deref(),
    );
    let _ = write_entry(&crash_dir(data_dir), &entry);
}

/// Write `entry` into the armed directory when local capture is enabled.
fn write_if_enabled(entry: &CrashEntry) {
    let Some(dir) = DIR.get() else { return };
    let Some(data_dir) = dir.parent() else { return };
    if !is_enabled(data_dir) {
        return;
    }
    let _ = write_entry(dir, entry);
}

// ─────────────────────────────────────────────────────────────────────────────
//   The opt-in flag (unchanged semantics, new home)
// ─────────────────────────────────────────────────────────────────────────────

fn flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FLAG_FILE)
}

/// The crash directory inside an app-data dir. Public so the commands can read
/// a ring without the `OnceLock` being armed (tests, and the Settings card).
pub fn crash_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CRASH_DIR)
}

/// Whether the user has opted in to LOCAL crash capture (default off).
///
/// This is not, and must never be treated as, consent to send anything. E5's
/// consent machine is a separate three-state record; this flag only decides
/// whether files are written on this machine at all.
pub fn is_enabled(data_dir: &Path) -> bool {
    std::fs::read_to_string(flag_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
        .unwrap_or(false)
}

/// Persist the opt-in choice.
pub fn set_enabled(data_dir: &Path, enabled: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let body = serde_json::json!({ "enabled": enabled }).to_string();
    std::fs::write(flag_path(data_dir), body)
}

// ─────────────────────────────────────────────────────────────────────────────
//   The ring
// ─────────────────────────────────────────────────────────────────────────────

/// Write one record into `dir` and prune the ring back to [`RING_MAX`].
///
/// Atomic temp+rename: a reader — the E5 payload builder, or a support person
/// with a Finder window — must never see a half-written file, least of all one
/// written while the process was dying.
fn write_entry(dir: &Path, entry: &CrashEntry) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let name = entry_filename(entry.at.max(0) as u128, SEQ.fetch_add(1, Ordering::Relaxed));
    let body = serde_json::to_string_pretty(entry)
        .unwrap_or_else(|_| "{\"schema\":1,\"kind\":\"other\"}".to_string());
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, dir.join(&name))?;
    prune(dir, RING_MAX);
    Ok(())
}

/// The filename for a record. Zero-padded so a plain lexicographic sort IS
/// chronological order — which is what lets [`prune`] pick victims without
/// stat-ing anything, and what makes the directory readable in Finder.
fn entry_filename(millis: u128, seq: u32) -> String {
    format!("{PREFIX}{millis:015}-{seq:04}.json")
}

/// Delete the oldest records until at most `cap` remain. Best-effort: a file we
/// cannot remove is left alone rather than retried.
fn prune(dir: &Path, cap: usize) {
    let mut names = record_names(dir);
    names.sort();
    for victim in ring_victims(&names, cap) {
        let _ = std::fs::remove_file(dir.join(victim));
    }
}

fn record_names(dir: &Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(PREFIX) && n.ends_with(".json"))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Which of `names` (already sorted oldest-first) must go so at most `cap`
/// remain. Pure, so the ring's arithmetic is provable without a filesystem.
fn ring_victims(names: &[String], cap: usize) -> Vec<String> {
    if names.len() <= cap {
        return Vec::new();
    }
    names[..names.len() - cap].to_vec()
}

/// Every record in `dir`, oldest first. A stray or torn file is skipped rather
/// than failing the read.
pub fn read_entries(dir: &Path) -> Vec<CrashEntry> {
    let mut names = record_names(dir);
    names.sort();
    names
        .iter()
        .filter_map(|n| std::fs::read_to_string(dir.join(n)).ok())
        .filter_map(|s| serde_json::from_str::<CrashEntry>(&s).ok())
        .collect()
}

/// How many records the ring holds.
pub fn count(dir: &Path) -> usize {
    record_names(dir).len()
}

/// Delete every record. The directory itself is kept so the next write does not
/// have to recreate it.
pub fn clear(dir: &Path) -> std::io::Result<()> {
    for name in record_names(dir) {
        std::fs::remove_file(dir.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: CrashKind, msg: &str, at: i64) -> CrashEntry {
        build_entry(kind, msg, Some("src/lib.rs:1:1"), None, at, None)
    }

    fn enabled_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        set_enabled(dir.path(), true).expect("flag");
        dir
    }

    // ── the opt-in flag ──────────────────────────────────────────────────────

    #[test]
    fn the_flag_defaults_off_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!is_enabled(dir.path()), "local capture is off by default");
        set_enabled(dir.path(), true).expect("set");
        assert!(is_enabled(dir.path()));
        set_enabled(dir.path(), false).expect("set");
        assert!(!is_enabled(dir.path()));
        // A hand-mangled flag file reads as off, never as on.
        std::fs::write(dir.path().join(FLAG_FILE), "not json").expect("write");
        assert!(!is_enabled(dir.path()));
    }

    // ── the record ───────────────────────────────────────────────────────────

    #[test]
    fn a_record_is_scrubbed_and_capped_before_it_reaches_disk() {
        let entry = build_entry(
            CrashKind::WebviewError,
            &format!(
                "kunne ikke laste /Users/ola/Musikk/salme.mp4 {}",
                "æ".repeat(MESSAGE_MAX_CHARS)
            ),
            Some("/Users/ola/dev/sundaystage/src/x.tsx:9:1"),
            Some("workspace"),
            42,
            Some("/Users/ola"),
        );
        assert!(!entry.message.contains("/Users/"), "{}", entry.message);
        assert!(!entry.message.contains("salme"), "{}", entry.message);
        assert!(entry.message.contains("<path>"));
        assert_eq!(entry.message.chars().count(), MESSAGE_MAX_CHARS + 1);
        assert!(entry.message.ends_with('…'));
        let loc = entry.location.expect("location kept");
        assert!(!loc.contains("/Users/"), "{loc}");
        assert_eq!(entry.task.as_deref(), Some("workspace"));
        assert_eq!(entry.schema, RECORD_SCHEMA);
        assert_eq!(entry.app_version, app_version());
        assert!(!entry.backtrace_present, "no backtrace outside the hook");
    }

    #[test]
    fn an_empty_location_or_task_becomes_none_not_an_empty_string() {
        let entry = build_entry(CrashKind::Other, "boom", Some(""), Some(""), 1, None);
        assert_eq!(entry.location, None);
        assert_eq!(entry.task, None);
    }

    #[test]
    fn the_panic_hook_builds_a_record_without_panicking() {
        // `PanicHookInfo` cannot be constructed outside std, so drive the real
        // hook: catch a panic and let a hook we install for the duration build
        // the record. This also proves the hook path itself does not panic.
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<CrashEntry>));
        let sink = std::sync::Arc::clone(&captured);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let entry = build_panic_entry(info, Some("/Users/ola"), 7, true);
            // The hook is process-global while installed and the harness runs
            // other tests (some of which panic on purpose) in parallel threads.
            // Only keep OURS, or the assertion would be decided by scheduling.
            if entry.message.contains("deliberate test panic") {
                if let Ok(mut g) = sink.lock() {
                    *g = Some(entry);
                }
            }
        }));
        let _ = std::panic::catch_unwind(|| panic!("deliberate test panic: /Users/ola/x.wav"));
        std::panic::set_hook(previous);

        let entry = captured
            .lock()
            .expect("sink lock")
            .clone()
            .expect("the hook must have built a record");
        assert_eq!(entry.kind, CrashKind::Panic);
        assert_eq!(entry.at, 7);
        assert!(entry.backtrace_present);
        // The operator's path is gone but the panic's identity survives.
        assert!(entry.message.starts_with("deliberate test panic:"));
        assert!(!entry.message.contains("/Users/"), "{}", entry.message);
        // The location is THIS file, with the crate-relative path std reports —
        // relative paths must survive scrubbing or the field is useless.
        let loc = entry
            .location
            .expect("a `panic!` always carries a location");
        assert!(loc.contains("crash_ring.rs"), "{loc}");
    }

    // ── the ring ─────────────────────────────────────────────────────────────

    #[test]
    fn ring_victims_keeps_the_newest_cap_entries() {
        let names: Vec<String> = (0..5).map(|i| entry_filename(i, 0)).collect();
        assert!(ring_victims(&names, 5).is_empty());
        assert!(ring_victims(&names, 9).is_empty());
        assert_eq!(ring_victims(&names, 3), names[..2].to_vec());
        assert_eq!(ring_victims(&names, 0), names);
        assert!(ring_victims(&[], 3).is_empty());
    }

    #[test]
    fn record_filenames_sort_chronologically() {
        // Zero-padding is the whole point: without it "999" would sort after
        // "1000" and the ring would evict the NEWEST record.
        let mut names = vec![
            entry_filename(1_000, 0),
            entry_filename(999, 0),
            entry_filename(1_000, 1),
        ];
        names.sort();
        assert_eq!(
            names,
            vec![
                entry_filename(999, 0),
                entry_filename(1_000, 0),
                entry_filename(1_000, 1),
            ]
        );
    }

    #[test]
    fn the_ring_caps_at_twenty_and_drops_the_oldest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = crash_dir(dir.path());
        for i in 0..(RING_MAX + 7) {
            write_entry(&path, &sample(CrashKind::Panic, &format!("p{i}"), i as i64))
                .expect("write");
        }
        let kept = read_entries(&path);
        assert_eq!(kept.len(), RING_MAX);
        assert_eq!(count(&path), RING_MAX);
        // Oldest first; the survivors are the newest RING_MAX.
        assert_eq!(kept.first().expect("first").message, "p7");
        assert_eq!(
            kept.last().expect("last").message,
            format!("p{}", RING_MAX + 6)
        );
        // An atomic write leaves no temp files behind.
        let leftovers: Vec<_> = std::fs::read_dir(&path)
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "atomic write left .tmp files");
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = crash_dir(dir.path());
        let entry = sample(CrashKind::UnhandledRejection, "boom", 5);
        write_entry(&path, &entry).expect("write");
        assert_eq!(read_entries(&path), vec![entry]);
        // camelCase on the wire, so E4's schema needs no remapping.
        let names = record_names(&path);
        let body = std::fs::read_to_string(path.join(&names[0])).expect("read");
        assert!(body.contains("\"appVersion\""), "{body}");
        assert!(body.contains("\"backtracePresent\""), "{body}");
        assert!(body.contains("\"unhandled_rejection\""), "{body}");
    }

    #[test]
    fn an_absent_or_littered_directory_reads_as_no_records() {
        assert!(read_entries(Path::new("/definitely/not/a/dir")).is_empty());
        assert_eq!(count(Path::new("/definitely/not/a/dir")), 0);
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("notes.txt"), b"hello").expect("write");
        std::fs::write(dir.path().join("crash-bad.json"), b"{not json").expect("write");
        assert!(read_entries(dir.path()).is_empty());
        // The torn file still counts as present (the Settings card says "1"),
        // and clearing removes it.
        assert_eq!(count(dir.path()), 1);
        clear(dir.path()).expect("clear");
        assert_eq!(count(dir.path()), 0);
        // The unrelated file is untouched — clearing crash reports must not
        // sweep the app-data directory.
        assert!(dir.path().join("notes.txt").exists());
    }

    // ── the opt-in gate on the write path ────────────────────────────────────

    #[test]
    fn writing_is_gated_on_the_opt_in_flag() {
        // `write_if_enabled` reads the flag beside the crash directory. Drive
        // the same decision explicitly rather than arming the process-global
        // OnceLock, which other tests share.
        let dir = enabled_dir();
        let ring = crash_dir(dir.path());
        assert!(is_enabled(dir.path()));
        write_entry(&ring, &sample(CrashKind::Panic, "kept", 1)).expect("write");
        assert_eq!(count(&ring), 1);

        set_enabled(dir.path(), false).expect("flag off");
        assert!(!is_enabled(dir.path()));
        // With the flag off `write_if_enabled` returns before `write_entry`.
        assert_eq!(count(&ring), 1, "no new record while opted out");
    }

    #[test]
    fn arming_is_idempotent_and_the_dir_hangs_off_app_data() {
        // `arm` uses a process-global OnceLock, so a second call must be a
        // harmless no-op rather than a panic.
        let dir = tempfile::tempdir().expect("tempdir");
        arm(dir.path());
        arm(dir.path());
        let armed = super::dir().expect("armed");
        assert!(armed.ends_with(CRASH_DIR));
    }
}
