//! Phase 5.2 — spawn + supervise the crash-isolated output processes.
//!
//! One `sundaystage-output` process per assigned display. The supervisor:
//!
//!   * binds a deterministic local-IPC endpoint per output (see
//!     [`crate::output::ipc::endpoint_path`]) and spawns the child pointing
//!     at it;
//!   * forwards every [`OutputMessage`] (renders from the live engine,
//!     250 ms heartbeats) and reads the child's [`OutputAck`]s;
//!   * re-sends the **current frame** to a child the moment it (re)connects,
//!     so a restarted process is never blank;
//!   * restarts a child that dies (crash isolation works both ways);
//!   * on graceful shutdown sends [`OutputMessage::Shutdown`] then kills.
//!
//! The inverse direction — the *main* app dying — is the child's job: its
//! watchdog holds the last frame on heartbeat loss and the process outlives
//! its parent (verified headlessly in `tests/output_isolation.rs`).
//!
//! Stale children from a *crashed* previous main process are reaped via a
//! pidfile in [`pidfile_dir`] before respawning, so a relaunch never stacks
//! two full-screen windows on the same projector.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

// Poison-free locks: a panic anywhere in the supervisor must never wedge the
// render/heartbeat path that keeps the projector alive.
use parking_lot::Mutex;

use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::output::ipc::{endpoint_path, safe_tag, IpcListener};
use crate::output::{OutputAck, OutputMessage, DEFAULT_TIMEOUT_MS};
use crate::services::live_session::LiveFrame;
use crate::telemetry::quality::{LiveSafe, QualityCollector};
use crate::telemetry::scrub;

/// How often the supervisor proves the main process is alive to each child.
pub const HEARTBEAT_MS: u64 = 250;
/// How long a dead child waits before being respawned.
const RESTART_BACKOFF_MS: u64 = 500;
/// How long we wait for a spawned child to connect before retrying.
const CONNECT_TIMEOUT_MS: u64 = 10_000;
/// How long graceful [`shutdown`](OutputSupervisor::shutdown) waits for the
/// supervision tasks (and their children) to exit cleanly before aborting them.
const SHUTDOWN_GRACE_MS: u64 = 500;

/// Directory (under the app-local data dir) holding one pidfile per running
/// output child.
const PIDFILE_DIR: &str = "pidfiles";
/// Filename shape: `sundaystage-<label>.pid`. The prefix is what the scan in
/// [`stale_pidfiles_present`] matches on, and it is kept from the old
/// next-to-the-socket layout so the transition scan below needs no second rule.
const PIDFILE_PREFIX: &str = "sundaystage-";
const PIDFILE_SUFFIX: &str = ".pid";

/// Everything needed to spawn one output process.
#[derive(Debug, Clone)]
pub struct OutputSpec {
    /// Window label (`output-main-0`…). Also keys the IPC endpoint + pidfile.
    pub label: String,
    /// Monitor geometry for the full-screen window (ignored when `headless`).
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Run the child without a window (CI / integration tests).
    pub headless: bool,
    /// Path to the saved `output_appearance.json` the child serves to its
    /// renderer (the child has no DB).
    pub appearance_file: Option<PathBuf>,
}

/// A point-in-time view of one supervised child, for status UIs and tests.
#[derive(Debug, Clone)]
pub struct ChildStatus {
    pub label: String,
    pub pid: Option<u32>,
    pub connected: bool,
    pub restarts: u32,
    pub last_acked_seq: u64,
}

struct ChildShared {
    label: String,
    pid: Mutex<Option<u32>>,
    connected: AtomicBool,
    restarts: AtomicU64,
    last_acked_seq: AtomicU64,
    /// Unix ms this child last went from connected to not — the input to the
    /// hold-last-frame detection in the heartbeat pump. 0 = never.
    disconnected_at: AtomicU64,
    /// Whether we have already counted this disconnection as a hold, so one
    /// gap produces one quality signal rather than four per second.
    counted_hold: AtomicBool,
}

struct Inner {
    /// Fan-out of protocol messages to every per-child pump.
    tx: broadcast::Sender<OutputMessage>,
    /// The frame currently meant to be on screen — re-sent on (re)connect.
    last_frame: Mutex<Option<LiveFrame>>,
    seq: AtomicU64,
    shutting_down: AtomicBool,
    children: Mutex<Vec<Arc<ChildShared>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// The app-local data dir — the root [`pidfile_dir`] hangs off. Carried
    /// here rather than resolved from a path API so the supervisor stays
    /// testable without an app handle (and so the reaper and the startup scan
    /// cannot drift onto two different directories).
    data_dir: PathBuf,
    /// E3 — where restarts, connect timeouts, held frames and cue latencies go.
    /// Every call on it is an atomic increment (see `telemetry::quality`), so
    /// the render path stays exactly as cheap as it was.
    telemetry: Arc<QualityCollector>,
}

/// Supervises the set of output processes for the current "outputs open"
/// session. Create with [`OutputSupervisor::start`]; drop or
/// [`shutdown`](Self::shutdown) to tear down (children are `kill_on_drop`).
pub struct OutputSupervisor {
    inner: Arc<Inner>,
}

/// Resolve the output-process binary: an explicit override (tests, dev
/// tooling) or a sibling of the running executable — `sundaystage-output` in
/// a cargo target dir (dev), `output-process` inside the installed bundle
/// (the `externalBin` name; see build.rs for why it differs). Empty files are
/// rejected: build.rs maintains an empty externalBin *placeholder* for plain
/// cargo builds, and spawning it would fail confusingly mid-service — better
/// to fall back to the in-process windows.
pub fn output_binary_path() -> Option<PathBuf> {
    fn usable(p: &Path) -> bool {
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    }
    if let Ok(p) = std::env::var("SUNDAYSTAGE_OUTPUT_BIN") {
        let p = PathBuf::from(p);
        return usable(&p).then_some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let ext = if cfg!(windows) { ".exe" } else { "" };
    ["sundaystage-output", "output-process"]
        .iter()
        .map(|name| dir.join(format!("{name}{ext}")))
        .find(|p| usable(p))
}

/// Where output pidfiles live, given the app-local data dir.
///
/// A REAL directory on both platforms, and deliberately **not** derived from
/// the IPC endpoint. It used to be: `pidfile_path` took [`endpoint_path`] and
/// swapped the extension, which on Windows produced
/// `\\.\pipe\sundaystage-<label>.pid` — not a filesystem location, so the file
/// could not be written, read, deleted or scanned. Stale-child reaping and the
/// startup "did we crash last time?" signal were both silently inert there.
///
/// This function has no `cfg` branch, which is the point: the layout a macOS
/// test observes IS the Windows layout.
pub fn pidfile_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(PIDFILE_DIR)
}

/// The pidfile name for one output child. Pure; shares [`safe_tag`] with the
/// endpoint so both names come from the same sanitised label.
fn pidfile_name(label: &str) -> String {
    format!("{PIDFILE_PREFIX}{}{PIDFILE_SUFFIX}", safe_tag(label))
}

fn pidfile_path(data_dir: &Path, label: &str) -> PathBuf {
    pidfile_dir(data_dir).join(pidfile_name(label))
}

// ── transition: the pre-0.6 unix location ───────────────────────────────────
//
// Up to and including 0.5.0, unix pidfiles sat next to the socket in the system
// temp dir (`<tmp>/sundaystage-<label>.pid`); on Windows they never existed at
// all. A machine that crashed under the old layout still has a child holding a
// frame and a pidfile in the OLD place, so for ONE release both locations are
// reaped and scanned.
//
// REMOVE the `legacy_*` items and their two call sites in 0.7.0: by then every
// install has launched at least once on 0.6.x, and that launch deleted any
// old-location pidfile it found (`reap_pidfile_at` removes the file whether or
// not the pid was still alive).
#[cfg(unix)]
fn legacy_pidfile_dir() -> PathBuf {
    std::env::temp_dir()
}

#[cfg(unix)]
fn legacy_pidfile_path(label: &str) -> PathBuf {
    legacy_pidfile_dir().join(pidfile_name(label))
}

/// The executable names an output child can be running under: the cargo bin in
/// a dev tree, the `externalBin` inside the installed bundle. Nothing else on
/// the machine may ever be signalled by the reaper.
const OUTPUT_PROCESS_NAMES: [&str; 2] = ["sundaystage-output", "output-process"];

/// Shortest prefix of an [`OUTPUT_PROCESS_NAMES`] entry accepted as a match.
/// Linux's `ps -o comm=` truncates at 15 characters, so `sundaystage-output`
/// comes back as `sundaystage-out`; a floor keeps that from turning into "any
/// short name matches".
const MIN_NAME_MATCH: usize = 12;

/// What one pidfile records.
///
/// The pid alone is not enough to authorise `kill -9`: pids are recycled, and
/// (since 0.6) the file lives in the app-data dir, which nothing purges. So the
/// file also carries a fingerprint of the process it was written for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PidRecord {
    pid: u32,
    /// When the OS says that process started. `None` in a file written by an
    /// older build, or on a platform where we do not ask.
    started: Option<String>,
}

/// How the OS identifies whatever holds a pid right now.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessIdentity {
    /// The executable it is running, as `ps`/`tasklist` reports it.
    name: String,
    /// When it started, when that is available.
    started: Option<String>,
}

/// The pidfile's contents for a child we just spawned.
fn pidfile_contents(record: &PidRecord) -> String {
    match &record.started {
        Some(started) => format!("{}:{started}", record.pid),
        None => record.pid.to_string(),
    }
}

/// Parse a pidfile. `0` is filtered out rather than passed on: on unix
/// `kill -9 0` signals every process in our OWN process group, which is the
/// whole app. A pidfile can only read `0` if it was written from an unknown
/// child id, and no stale child is worth that blast radius.
///
/// A bare pid (the pre-0.6.1 shape) parses with no fingerprint, so an install
/// upgrading across this change still reaps the child its old build left.
fn parse_pidfile(contents: &str) -> Option<PidRecord> {
    let text = contents.trim();
    let (pid, started) = match text.split_once(':') {
        Some((pid, started)) => (pid, Some(started.trim())),
        None => (text, None),
    };
    let pid = pid.trim().parse::<u32>().ok().filter(|p| *p > 0)?;
    Some(PidRecord {
        pid,
        started: started.filter(|s| !s.is_empty()).map(str::to_string),
    })
}

/// Is this executable name one of ours?
fn is_output_process_name(name: &str) -> bool {
    let base = name.trim().rsplit(['/', '\\']).next().unwrap_or("").trim();
    let base = base.strip_suffix(".exe").unwrap_or(base);
    base.len() >= MIN_NAME_MATCH && OUTPUT_PROCESS_NAMES.iter().any(|n| n.starts_with(base))
}

/// Whether a pidfile authorises SIGKILL of whatever holds that pid NOW.
///
/// Pure, because this is the decision that must never be wrong in either
/// direction: killing a recycled pid takes down an unrelated app (or another
/// SundayStage's live output), and refusing to kill a real leftover leaves two
/// full-screen windows stacked on the projector.
///
///   * nothing there → nothing to kill;
///   * not one of OUR executables → never, whatever the file says;
///   * a recorded start time that disagrees with the live one → a different
///     process inherited the pid; leave it alone.
fn may_signal(record: &PidRecord, live: Option<&ProcessIdentity>) -> bool {
    let Some(live) = live else { return false };
    if !is_output_process_name(&live.name) {
        return false;
    }
    match (&record.started, &live.started) {
        (Some(recorded), Some(running)) => recorded == running,
        // A legacy file, or an OS we do not ask for a start time: the name is
        // the guard, and it is the guard this reaper never had at all before.
        _ => true,
    }
}

/// Ask the OS what is running as `pid`, without a `/proc` walk or a new crate.
///
/// One `ps` per reap and one per spawn, both off the render path. On Windows
/// only the image name is available this cheaply; [`may_signal`] degrades to
/// the name check there, which is exactly the guard Windows never had (it had
/// no pidfiles at all before 0.6).
#[cfg(unix)]
fn process_identity(pid: u32) -> Option<ProcessIdentity> {
    let name = ps_field(pid, "comm=")?;
    Some(ProcessIdentity {
        started: ps_field(pid, "lstart="),
        name,
    })
}

#[cfg(unix)]
fn ps_field(pid: u32, field: &str) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", field, "-p", &pid.to_string()])
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!value.is_empty()).then_some(value)
}

#[cfg(windows)]
fn process_identity(pid: u32) -> Option<ProcessIdentity> {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .ok()?;
    let line = String::from_utf8_lossy(&out.stdout);
    // No match prints `INFO: No tasks are running…` rather than an empty body.
    let name = line
        .lines()
        .find(|l| l.starts_with('"'))?
        .trim_start_matches('"')
        .split('"')
        .next()?
        .to_string();
    (!name.is_empty()).then_some(ProcessIdentity {
        name,
        started: None,
    })
}

/// Removes a pidfile when the scope that owns the child ends.
///
/// A `Drop` rather than a line at the exit, because [`run_child_once`] has
/// SEVEN ways out — clean shutdown, the child dying before it connects, the
/// connect timeout, a failed write to the child, a closed broadcast, a torn
/// read, and the `?` on the socket bind — and only the first one used to remove
/// the file. Every other path left a pidfile behind in a directory nothing
/// purges, so the NEXT launch reported `staleChildReaped` and a
/// `hold-last-frame` reason for a run that ended perfectly, and asked `kill -9`
/// for a pid the OS had long since handed to somebody else.
struct PidfileGuard {
    path: PathBuf,
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Record the running child so a crashed main process leaves a reapable trace,
/// and return the guard that removes the trace again on every way out.
/// Best-effort: failing to write costs the NEXT launch its reap, never this
/// service.
fn write_pidfile(data_dir: &Path, label: &str, pid: u32) -> PidfileGuard {
    let path = pidfile_path(data_dir, label);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let record = PidRecord {
        pid,
        started: process_identity(pid).and_then(|i| i.started),
    };
    let _ = std::fs::write(&path, pidfile_contents(&record));
    PidfileGuard { path }
}

/// Best-effort kill of a stale child left over from a crashed main process,
/// recorded in the pidfile. Never touches anything when the file is absent.
/// Returns whether a stale child was found — E3's `staleChildReaped` signal.
///
/// Both locations are checked and both are cleaned; see the transition note
/// above.
fn reap_stale_child(data_dir: &Path, label: &str) -> bool {
    let current = reap_pidfile_at(&pidfile_path(data_dir, label), label);
    #[cfg(unix)]
    let legacy = reap_pidfile_at(&legacy_pidfile_path(label), label);
    #[cfg(not(unix))]
    let legacy = false;
    // Not `||`: short-circuiting would leave the old-location file behind on a
    // machine that has one in both places.
    current || legacy
}

/// Kill whatever `pidfile` names — if, and only if, that pid still belongs to
/// the output child the file was written for ([`may_signal`]). Then delete the
/// file. Returns whether the file was there at all, which is the
/// `staleChildReaped` signal: a leftover file means the previous run never got
/// to clean up, whether or not its child is still around.
fn reap_pidfile_at(pidfile: &Path, label: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(pidfile) else {
        return false;
    };
    if let Some(record) = parse_pidfile(&contents) {
        let pid = record.pid;
        if may_signal(&record, process_identity(pid).as_ref()) {
            tracing::warn!(label, pid, "reaping a stale output process");
            #[cfg(unix)]
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
            #[cfg(windows)]
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .status();
        } else {
            // Gone, or recycled onto something that is not ours. Either way the
            // file is stale bookkeeping, not a licence to signal.
            tracing::info!(
                label,
                pid,
                "a leftover output pidfile no longer names our child — not signalling"
            );
        }
    }
    let _ = std::fs::remove_file(pidfile);
    true
}

/// Whether any output pidfile is still on disk — i.e. a child from a previous
/// process may still be holding its last frame on a projector.
///
/// Read at startup by [`crate::telemetry::quality::reconstruct_previous_session`]
/// to decide the `staleChildReaped` flag on the reconstructed row.
///
/// Works on **both** platforms since the pidfiles moved off the IPC endpoint
/// into [`pidfile_dir`]; on unix it also covers the pre-0.6 temp-dir location
/// for one release (see the transition note above).
pub fn stale_pidfiles_present(data_dir: &Path) -> bool {
    if pidfiles_in(&pidfile_dir(data_dir)) {
        return true;
    }
    // A scan has no side effects, so unlike the reaper this one may
    // short-circuit.
    #[cfg(unix)]
    {
        pidfiles_in(&legacy_pidfile_dir())
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Does `dir` hold any `sundaystage-*.pid`? A missing directory is simply "no".
fn pidfiles_in(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.filter_map(Result::ok).any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.starts_with(PIDFILE_PREFIX) && name.ends_with(PIDFILE_SUFFIX)
    })
}

impl OutputSupervisor {
    /// Spawn + supervise one child per spec. Must be called on a tokio runtime
    /// (Tauri's async runtime or `#[tokio::test]`).
    ///
    /// `data_dir` is the app-local data dir (`AppState::data_dir`); pidfiles go
    /// in [`pidfile_dir`] under it, so a crashed run's children are reapable on
    /// the next launch on every platform.
    pub fn start(
        binary: PathBuf,
        data_dir: PathBuf,
        specs: Vec<OutputSpec>,
        telemetry: Arc<QualityCollector>,
    ) -> Self {
        let (tx, _) = broadcast::channel::<OutputMessage>(64);
        let inner = Arc::new(Inner {
            tx,
            last_frame: Mutex::new(None),
            seq: AtomicU64::new(0),
            shutting_down: AtomicBool::new(false),
            children: Mutex::new(Vec::new()),
            tasks: Mutex::new(Vec::new()),
            data_dir,
            telemetry,
        });

        let mut tasks = Vec::new();
        for spec in specs {
            let shared = Arc::new(ChildShared {
                label: spec.label.clone(),
                pid: Mutex::new(None),
                connected: AtomicBool::new(false),
                restarts: AtomicU64::new(0),
                last_acked_seq: AtomicU64::new(0),
                disconnected_at: AtomicU64::new(0),
                counted_hold: AtomicBool::new(false),
            });
            inner.children.lock().push(shared.clone());
            tasks.push(tokio::spawn(supervise_child(
                inner.clone(),
                binary.clone(),
                spec,
                shared,
            )));
        }
        // The heartbeat pump: one timer feeds every child via the broadcast,
        // and — E3 — notices when a child has been silent long enough that its
        // own watchdog must now be holding the last frame.
        {
            let inner = inner.clone();
            tasks.push(tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(HEARTBEAT_MS));
                loop {
                    tick.tick().await;
                    if inner.shutting_down.load(Ordering::SeqCst) {
                        break;
                    }
                    let now = crate::db::now_ms();
                    let _ = inner.tx.send(OutputMessage::Heartbeat { at: now });
                    note_held_frames(&inner, now);
                }
            }));
        }
        *inner.tasks.lock() = tasks;
        Self { inner }
    }

    /// Push a new frame to every output process. Sync + lock-cheap so the
    /// live dispatch path stays O(1); returns the assigned `seq`.
    pub fn render(&self, frame: LiveFrame) -> u64 {
        let seq = self.inner.seq.fetch_add(1, Ordering::SeqCst) + 1;
        *self.inner.last_frame.lock() = Some(frame.clone());
        let _ = self.inner.tx.send(OutputMessage::Render { frame, seq });
        // Two relaxed atomic stores — the dispatch→ACK stopwatch starts here.
        self.inner.telemetry.note_render(seq);
        seq
    }

    /// Graceful teardown: tell children to shut down, give them a bounded
    /// window to exit cleanly, then reap whatever remains.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        let _ = self.inner.tx.send(OutputMessage::Shutdown);
        // Wait up to SHUTDOWN_GRACE_MS for the supervision tasks to finish on
        // their own — each one forwards the Shutdown, waits on the child, kills
        // it and returns. Joining them (rather than a flat sleep) lets a clean
        // exit complete in a few ms, while still capping the worst case so a
        // wedged child can't hang app teardown. Whatever hasn't finished by the
        // deadline is aborted (its child is `kill_on_drop`).
        let tasks: Vec<JoinHandle<()>> = self.inner.tasks.lock().drain(..).collect();
        let aborts: Vec<_> = tasks.iter().map(|t| t.abort_handle()).collect();
        let join_all = async {
            for t in tasks {
                let _ = t.await;
            }
        };
        if tokio::time::timeout(Duration::from_millis(SHUTDOWN_GRACE_MS), join_all)
            .await
            .is_err()
        {
            // Grace expired with tasks still alive — abort the stragglers; the
            // `kill_on_drop` children die when their command future is dropped.
            tracing::warn!("output supervisor shutdown grace expired — aborting stragglers");
            for a in aborts {
                a.abort();
            }
        }
    }

    /// Status snapshot (operator UI + integration tests).
    pub fn status(&self) -> Vec<ChildStatus> {
        self.inner
            .children
            .lock()
            .iter()
            .map(|c| ChildStatus {
                label: c.label.clone(),
                pid: *c.pid.lock(),
                connected: c.connected.load(Ordering::SeqCst),
                restarts: c.restarts.load(Ordering::SeqCst) as u32,
                last_acked_seq: c.last_acked_seq.load(Ordering::SeqCst),
            })
            .collect()
    }

    /// True until [`shutdown`](Self::shutdown) is called.
    pub fn is_running(&self) -> bool {
        !self.inner.shutting_down.load(Ordering::SeqCst)
    }
}

/// Count the outputs whose link has been down long enough that the child's own
/// watchdog has entered hold-last-frame — the congregation is looking at a
/// frozen slide.
///
/// **This is an inference, and an honest one.** The watchdog itself
/// ([`crate::output::Watchdog`]) runs in the CHILD, and by the time it fires
/// there is by definition no link to report over: a hold and a link loss are
/// the same event seen from the two ends. So the parent measures the only thing
/// it can — how long an output has had nobody connected — and calls a gap
/// longer than [`DEFAULT_TIMEOUT_MS`] a hold, because past that instant a
/// living child IS holding. A child that died instead of holding is counted as
/// a restart as well, and the pair reads correctly together.
fn note_held_frames(inner: &Arc<Inner>, now: i64) {
    for child in inner.children.lock().iter() {
        if child.connected.load(Ordering::SeqCst) {
            continue;
        }
        let since = child.disconnected_at.load(Ordering::SeqCst) as i64;
        if since == 0 || now.saturating_sub(since) <= DEFAULT_TIMEOUT_MS {
            continue;
        }
        // One gap, one signal.
        if !child.counted_hold.swap(true, Ordering::SeqCst) {
            inner.telemetry.note_watchdog_hold();
            tracing::warn!(
                label = %child.label,
                "output link down past the watchdog timeout — the child is holding its last frame"
            );
        }
    }
}

/// One child's supervision loop: bind → spawn → pump → (on death) respawn.
async fn supervise_child(
    inner: Arc<Inner>,
    binary: PathBuf,
    spec: OutputSpec,
    shared: Arc<ChildShared>,
) {
    let socket = endpoint_path(&spec.label);
    // A previous *crashed* main app may have left a child holding the last
    // frame on this very display — reap it before we put a new one there.
    if reap_stale_child(&inner.data_dir, &spec.label) {
        inner.telemetry.note_stale_child_reaped();
    }
    mark_disconnected(&shared);

    loop {
        if inner.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        match run_child_once(&inner, &binary, &spec, &shared, &socket).await {
            Ok(ChildExit::Shutdown) => return,
            Ok(ChildExit::Died) => {
                if inner.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                shared.restarts.fetch_add(1, Ordering::SeqCst);
                inner.telemetry.note_output_restart();
                tracing::warn!(
                    label = %spec.label,
                    "output process died — restarting (hold-last-frame covered the gap)"
                );
            }
            Err(e) => {
                if inner.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                shared.restarts.fetch_add(1, Ordering::SeqCst);
                inner.telemetry.note_output_restart();
                // The error is SCRUBBED: an `io::Error` from binding the local
                // endpoint or spawning the binary can name a filesystem
                // location, and this string reaches the uploadable log tail.
                tracing::error!(
                    label = %spec.label,
                    reason = %scrub::for_log(&e.to_string()),
                    "output process failed — retrying"
                );
            }
        }
        mark_disconnected(&shared);
        tokio::time::sleep(Duration::from_millis(RESTART_BACKOFF_MS)).await;
    }
}

/// Record that a child's link is down, starting the hold-detection clock. Only
/// the FIRST call after a connection starts the clock, so a restart loop does
/// not keep resetting it and hiding a long gap.
fn mark_disconnected(shared: &ChildShared) {
    let was_connected = shared.connected.swap(false, Ordering::SeqCst);
    if was_connected || shared.disconnected_at.load(Ordering::SeqCst) == 0 {
        shared
            .disconnected_at
            .store(crate::db::now_ms().max(0) as u64, Ordering::SeqCst);
    }
}

/// Record that a child is connected again: the gap is over and the next one
/// gets its own signal.
fn mark_connected(shared: &ChildShared) {
    shared.disconnected_at.store(0, Ordering::SeqCst);
    shared.counted_hold.store(false, Ordering::SeqCst);
    shared.connected.store(true, Ordering::SeqCst);
}

enum ChildExit {
    /// We told it to shut down (or the supervisor is closing).
    Shutdown,
    /// It died on its own — respawn.
    Died,
}

async fn run_child_once(
    inner: &Arc<Inner>,
    binary: &PathBuf,
    spec: &OutputSpec,
    shared: &Arc<ChildShared>,
    socket: &PathBuf,
) -> std::io::Result<ChildExit> {
    let mut listener = IpcListener::bind(socket)?;

    let mut cmd = tokio::process::Command::new(binary);
    cmd.arg("--socket")
        .arg(socket)
        .arg("--label")
        .arg(&spec.label)
        .arg("--position")
        .arg(format!("{},{}", spec.x, spec.y))
        .arg("--size")
        .arg(format!("{}x{}", spec.width, spec.height));
    if spec.headless {
        cmd.arg("--headless");
    }
    if let Some(f) = &spec.appearance_file {
        cmd.arg("--appearance-file").arg(f);
    }
    // Children must die with a *graceful* main-app exit (drop), but survive a
    // crash (no drop runs) — exactly the isolation contract.
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn()?;
    *shared.pid.lock() = child.id();
    // No id (the child has already been waited on) means no pidfile: a file
    // naming pid `0` would be worse than none at all — see [`parse_pidfile`].
    //
    // The guard is what removes it again. Holding it here means the file lives
    // exactly as long as this child does, on EVERY way out of this function
    // (and on an abort of the whole task, which drops these locals too) — not
    // just on the graceful-shutdown branch that used to own the only
    // `remove_file` call.
    let _pidfile = child
        .id()
        .map(|pid| write_pidfile(&inner.data_dir, &spec.label, pid));

    // Wait for the child to connect (or die trying).
    let stream = tokio::select! {
        accepted = listener.accept() => accepted?,
        status = child.wait() => {
            tracing::warn!(
                label = %spec.label,
                exit = ?status.as_ref().ok().and_then(|s| s.code()),
                "output exited before connecting"
            );
            return Ok(ChildExit::Died);
        }
        _ = tokio::time::sleep(Duration::from_millis(CONNECT_TIMEOUT_MS)) => {
            inner.telemetry.note_connect_timeout();
            tracing::error!(label = %spec.label, "output never connected — killing");
            let _ = child.kill().await;
            return Ok(ChildExit::Died);
        }
    };
    mark_connected(shared);
    let (mut reader, mut writer) = stream.into_split();

    // First thing on (re)connect: put the current frame on screen.
    let resend = inner.last_frame.lock().clone();
    if let Some(frame) = resend {
        let seq = inner.seq.load(Ordering::SeqCst);
        writer.write(&OutputMessage::Render { frame, seq }).await?;
    }

    let mut rx = inner.tx.subscribe();
    loop {
        tokio::select! {
            // Forward protocol traffic to the child.
            msg = rx.recv() => match msg {
                Ok(msg) => {
                    let is_shutdown = matches!(msg, OutputMessage::Shutdown);
                    if writer.write(&msg).await.is_err() {
                        return Ok(ChildExit::Died);
                    }
                    if is_shutdown {
                        // Give it a moment, then make sure. The pidfile goes
                        // with the guard, like it does on every other exit.
                        let _ = tokio::time::timeout(
                            Duration::from_millis(1_000), child.wait()).await;
                        let _ = child.kill().await;
                        return Ok(ChildExit::Shutdown);
                    }
                }
                // Lagged: skip to live — the next Render carries current state.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return Ok(ChildExit::Shutdown),
            },
            // ACKs back from the child.
            ack = reader.read::<OutputAck>() => match ack {
                Ok(Some(OutputAck::Rendered { seq, .. })) => {
                    shared.last_acked_seq.store(seq, Ordering::SeqCst);
                    // One compare-exchange: the dispatch→pixels stopwatch stops.
                    inner.telemetry.note_ack(seq);
                }
                Ok(Some(OutputAck::Error { message })) => {
                    // Never a dialog during a service — count it; the operator
                    // UI surfaces it as a toast via status polling.
                    //
                    // The child's text is NOT logged. It is
                    // `format!("bad message: {e}")` over a `serde_json` error
                    // about a Render frame, and a serde error quotes the value
                    // it choked on — which is a lyric. Law 2: the length is a
                    // finding, the content is not ours to write down.
                    inner.telemetry.note_dispatch_error();
                    // The length is computed OUTSIDE the macro: the tracing
                    // audit forbids the identifier `message` inside a call
                    // site, and that rule is worth more than the convenience.
                    let chars = message.chars().count();
                    tracing::warn!(
                        label = %spec.label,
                        chars,
                        "output child reported a render error"
                    );
                }
                Ok(None) | Err(_) => return Ok(ChildExit::Died),
            },
            // The process itself died (crash) — restart it.
            _ = child.wait() => return Ok(ChildExit::Died),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::quality::SessionOutcome;

    /// A pidfile the reaper will find but never signal on: `parse::<u32>`
    /// fails, so the file is cleaned up and nothing on this machine is killed.
    /// Every test that runs the reaper uses it — a test that left a REAL pid in
    /// the file would ask `kill -9` for it on the developer's machine.
    const UNPARSEABLE: &str = "not-a-pid";

    #[test]
    fn a_pidfile_is_a_real_file_under_the_data_dir_on_every_platform() {
        // Pure path logic: no `cfg`, no filesystem, no IPC endpoint. That is
        // exactly what makes this test meaningful for Windows while running on
        // macOS — the answer here IS the Windows layout.
        let data = PathBuf::from("app-data");
        let p = pidfile_path(&data, "output-main-0");

        assert_eq!(
            p,
            data.join(PIDFILE_DIR).join("sundaystage-output-main-0.pid")
        );
        assert_eq!(p.parent(), Some(pidfile_dir(&data).as_path()));
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("pid"));
        assert!(p.starts_with(&data), "pidfiles live under the app data dir");

        // The regression this file exists to prevent: a path derived from the
        // Windows endpoint (`\\.\pipe\…`) cannot be created, written or
        // scanned. Nothing pipe-shaped may appear in a pidfile path.
        let shown = p.to_string_lossy().into_owned();
        assert!(!shown.contains("pipe"), "{shown}");
        assert!(!shown.contains(".sock"), "{shown}");
    }

    #[test]
    fn pidfile_names_are_sanitised_exactly_like_the_endpoint() {
        // One label, one sanitisation rule — the endpoint and the pidfile must
        // never key the same child differently.
        assert_eq!(
            pidfile_name("output-main-0"),
            "sundaystage-output-main-0.pid"
        );
        assert_eq!(
            pidfile_name("output/main 0"),
            "sundaystage-output_main_0.pid"
        );
        let name = pidfile_name("a/b\\c:d");
        assert!(
            !name.contains(['/', '\\', ':']),
            "no path separator survives sanitisation: {name}"
        );
    }

    #[test]
    fn a_pidfile_is_written_scanned_and_reaped_in_the_data_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let label = "output-main-0";

        // Nothing yet — and this asks the data dir DIRECTLY rather than going
        // through `stale_pidfiles_present`, whose unix transition arm also
        // reads the shared system temp dir and so is not hermetic.
        assert!(!pidfiles_in(&pidfile_dir(dir.path())));
        assert!(!reap_stale_child(dir.path(), label));

        // Writing creates the directory (a fresh install has never had one).
        // The guard is kept alive: dropping it is what removes the file.
        let guard = write_pidfile(dir.path(), label, 4242);
        let path = pidfile_path(dir.path(), label);
        let written = std::fs::read_to_string(&path).expect("written");
        assert_eq!(
            parse_pidfile(&written).map(|r| r.pid),
            Some(4242),
            "the file names the child: {written}"
        );
        assert!(pidfiles_in(&pidfile_dir(dir.path())));
        assert!(stale_pidfiles_present(dir.path()));

        // …and reaping removes it and reports the find. (The pid is replaced
        // first: the reaper would otherwise ask about pid 4242 on this machine.)
        std::fs::write(&path, UNPARSEABLE).expect("overwrite");
        assert!(reap_stale_child(dir.path(), label));
        assert!(!path.exists());
        assert!(!pidfiles_in(&pidfile_dir(dir.path())));
        drop(guard);
    }

    /// The leak this file's `Drop` exists to close: a pidfile must not outlive
    /// the child it names. It used to, on every exit but the graceful one —
    /// and since the file moved out of the auto-purged temp dir into app data
    /// (0.6), every later launch on that machine read the leftover as "the
    /// previous run crashed with a child still holding a frame" and stamped
    /// `staleChildReaped` + `hold-last-frame` on a run that ended perfectly.
    #[test]
    fn the_pidfile_guard_removes_the_file_however_the_scope_ends() {
        let dir = tempfile::tempdir().expect("tempdir");
        let label = "output-main-0";
        let path = pidfile_path(dir.path(), label);

        {
            let _guard = write_pidfile(dir.path(), label, 4242);
            assert!(path.exists(), "the child is running, the trace is on disk");
        }
        assert!(!path.exists(), "leaving the scope removes it");

        // …including when the scope is left by a PANIC (an aborted supervision
        // task drops its locals the same way).
        let _ = std::panic::catch_unwind(|| {
            let _guard = write_pidfile(dir.path(), label, 4242);
            panic!("the supervision task died");
        });
        assert!(!path.exists(), "an unwind removes it too");
    }

    #[test]
    fn pid_zero_is_never_signalled() {
        // `kill -9 0` signals our entire process group — the whole app. Tested
        // on the pure decision rather than through the reaper on purpose: a
        // regression here must fail an assertion, not SIGKILL the test runner.
        assert_eq!(parse_pidfile("4242\n").map(|r| r.pid), Some(4242));
        assert_eq!(parse_pidfile("0"), None);
        assert_eq!(parse_pidfile(" 0 \n"), None);
        assert_eq!(parse_pidfile("0:Sat Aug  9 10:00:00 2026"), None);
        assert_eq!(parse_pidfile(UNPARSEABLE), None);
        assert_eq!(parse_pidfile(""), None);
        assert_eq!(parse_pidfile("-1"), None);
    }

    #[test]
    fn a_pidfile_records_the_process_it_was_written_for() {
        // Round-trip, including the pre-0.6.1 bare-pid shape an install
        // upgrading across this change still has on disk.
        let with_start = PidRecord {
            pid: 4242,
            started: Some("Sat Aug 9 10:00:00 2026".into()),
        };
        assert_eq!(
            parse_pidfile(&pidfile_contents(&with_start)),
            Some(with_start.clone())
        );
        let legacy = PidRecord {
            pid: 4242,
            started: None,
        };
        assert_eq!(pidfile_contents(&legacy), "4242");
        assert_eq!(parse_pidfile("4242"), Some(legacy));
    }

    /// The reaper's whole decision, on the pure function — because the wrong
    /// answer either SIGKILLs an unrelated app on the operator's machine or
    /// leaves two full-screen windows stacked on the projector.
    #[test]
    fn only_our_own_output_child_is_ever_signalled() {
        let started = "Sat Aug 9 10:00:00 2026";
        let record = PidRecord {
            pid: 4242,
            started: Some(started.into()),
        };
        let ours = |name: &str, at: Option<&str>| ProcessIdentity {
            name: name.into(),
            started: at.map(str::to_string),
        };

        // The child we wrote the file for.
        assert!(may_signal(
            &record,
            Some(&ours("/opt/SundayStage/sundaystage-output", Some(started)))
        ));
        // Nothing holds that pid any more.
        assert!(!may_signal(&record, None));
        // The pid was recycled onto something else entirely.
        assert!(!may_signal(
            &record,
            Some(&ours("/usr/bin/zoom", Some(started)))
        ));
        // …or onto a DIFFERENT output child (a second SundayStage): same name,
        // different start time.
        assert!(!may_signal(
            &record,
            Some(&ours("output-process", Some("Sat Aug 9 11:30:00 2026")))
        ));
        // A legacy file has no start time, so the name is the whole guard.
        let legacy = PidRecord {
            pid: 4242,
            started: None,
        };
        assert!(may_signal(&legacy, Some(&ours("output-process.exe", None))));
        assert!(!may_signal(&legacy, Some(&ours("Finder", None))));
    }

    #[test]
    fn the_executable_name_check_survives_truncation_but_not_strangers() {
        assert!(is_output_process_name("sundaystage-output"));
        assert!(is_output_process_name("/opt/app/sundaystage-output"));
        assert!(is_output_process_name(
            "C:\\Program Files\\App\\output-process.exe"
        ));
        // Linux `ps -o comm=` truncates at 15 characters.
        assert!(is_output_process_name("sundaystage-out"));
        // …but a short prefix is not an identification.
        assert!(!is_output_process_name("sundaystage"));
        assert!(!is_output_process_name("output"));
        assert!(!is_output_process_name(""));
        assert!(!is_output_process_name("sundaystage-outputter"));
        assert!(!is_output_process_name(
            "/System/Applications/Mail.app/Mail"
        ));
    }

    /// The identity probe runs against a real process — this one — so a
    /// platform whose `ps`/`tasklist` invocation drifted fails here rather
    /// than silently refusing to reap on an operator's machine.
    #[test]
    fn the_running_process_can_be_identified() {
        let me = process_identity(std::process::id()).expect("this process exists");
        assert!(!me.name.is_empty());
        // A pid that cannot exist has no identity, and therefore no licence.
        assert!(process_identity(u32::MAX).is_none());
    }

    /// Transition (remove with the `legacy_*` helpers in 0.7.0): a pidfile left
    /// by a 0.5.x crash sits next to the socket in the system temp dir, and the
    /// first launch after the upgrade must still find, reap and delete it.
    #[cfg(unix)]
    #[test]
    fn a_pidfile_from_the_old_location_is_still_reaped_and_scanned() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Process-unique so parallel tests (and stray files on the machine)
        // cannot collide in the shared temp dir.
        let label = format!("output-test-legacy-{}", std::process::id());
        let legacy = legacy_pidfile_path(&label);
        assert_eq!(legacy.parent(), Some(std::env::temp_dir().as_path()));
        assert_eq!(
            legacy.file_name().and_then(|n| n.to_str()),
            Some(format!("sundaystage-{label}.pid").as_str()),
            "the old-location name is unchanged, so one predicate covers both"
        );

        std::fs::write(&legacy, UNPARSEABLE).expect("write legacy pidfile");
        // The startup "crashed last time" signal sees it even though the new
        // location is empty…
        assert!(!pidfiles_in(&pidfile_dir(dir.path())));
        assert!(stale_pidfiles_present(dir.path()));
        // …and the reaper cleans it up, so the NEXT launch is quiet again.
        assert!(reap_stale_child(dir.path(), &label));
        assert!(!legacy.exists());
    }

    /// Both locations at once: the reaper must not short-circuit on the first
    /// one it finds, or the old file would survive the transition release.
    #[cfg(unix)]
    #[test]
    fn a_child_recorded_in_both_locations_is_reaped_from_both() {
        let dir = tempfile::tempdir().expect("tempdir");
        let label = format!("output-test-both-{}", std::process::id());
        let current = pidfile_path(dir.path(), &label);
        let legacy = legacy_pidfile_path(&label);
        std::fs::create_dir_all(current.parent().expect("parent")).expect("mkdir");
        std::fs::write(&current, UNPARSEABLE).expect("write");
        std::fs::write(&legacy, UNPARSEABLE).expect("write legacy");

        assert!(reap_stale_child(dir.path(), &label));
        assert!(!current.exists());
        assert!(
            !legacy.exists(),
            "the old-location file must be cleaned too"
        );
    }

    #[test]
    fn reaping_without_a_pidfile_is_a_noop_and_reports_nothing_found() {
        // Must never error or kill anything when no stale child exists — and
        // must not raise the `staleChildReaped` quality signal either.
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!reap_stale_child(dir.path(), "output-test-never-spawned"));
    }

    #[test]
    fn a_disconnected_child_is_counted_as_holding_exactly_once() {
        // The hold inference: past DEFAULT_TIMEOUT_MS with nobody connected,
        // the child's own watchdog is holding the last frame. One gap must
        // produce ONE quality signal, not one per 250 ms heartbeat.
        let telemetry = Arc::new(QualityCollector::new());
        let (tx, _) = broadcast::channel::<OutputMessage>(4);
        let shared = Arc::new(ChildShared {
            label: "output-main-0".into(),
            pid: Mutex::new(None),
            connected: AtomicBool::new(false),
            restarts: AtomicU64::new(0),
            last_acked_seq: AtomicU64::new(0),
            disconnected_at: AtomicU64::new(0),
            counted_hold: AtomicBool::new(false),
        });
        let inner = Arc::new(Inner {
            tx,
            last_frame: Mutex::new(None),
            seq: AtomicU64::new(0),
            shutting_down: AtomicBool::new(false),
            children: Mutex::new(vec![shared.clone()]),
            tasks: Mutex::new(Vec::new()),
            // No child is spawned here, so no pidfile is ever written.
            data_dir: PathBuf::new(),
            telemetry: telemetry.clone(),
        });

        // Nothing has disconnected yet.
        note_held_frames(&inner, 10_000);
        // A fresh disconnection: inside the timeout is not a hold.
        shared.disconnected_at.store(10_000, Ordering::SeqCst);
        note_held_frames(&inner, 10_000 + DEFAULT_TIMEOUT_MS);
        telemetry.finish_session(1, SessionOutcome::Clean);
        assert_eq!(
            drained_holds(&telemetry),
            0,
            "inside the timeout is a gap, not a hold"
        );

        // Past it, once — and staying past it does not count again.
        shared.disconnected_at.store(10_000, Ordering::SeqCst);
        shared.counted_hold.store(false, Ordering::SeqCst);
        for now in [12_001, 12_500, 20_000] {
            note_held_frames(&inner, now);
        }
        telemetry.finish_session(2, SessionOutcome::Clean);
        assert_eq!(drained_holds(&telemetry), 1);

        // A reconnection arms the next gap.
        mark_connected(&shared);
        assert!(shared.connected.load(Ordering::SeqCst));
        assert_eq!(shared.disconnected_at.load(Ordering::SeqCst), 0);
        assert!(!shared.counted_hold.load(Ordering::SeqCst));
        mark_disconnected(&shared);
        assert!(!shared.connected.load(Ordering::SeqCst));
        assert!(shared.disconnected_at.load(Ordering::SeqCst) > 0);
    }

    /// The `watchdog_holds` on the most recently finished session.
    fn drained_holds(telemetry: &QualityCollector) -> i64 {
        telemetry
            .take_buffered_rows()
            .last()
            .map_or(0, |r| r.watchdog_holds)
    }

    #[test]
    fn a_render_starts_the_latency_stopwatch() {
        // `render` is the live path: it must record the dispatch without
        // allocating or locking anything the telemetry owns.
        let telemetry = Arc::new(QualityCollector::new());
        let (tx, _rx) = broadcast::channel::<OutputMessage>(4);
        let inner = Arc::new(Inner {
            tx,
            last_frame: Mutex::new(None),
            seq: AtomicU64::new(0),
            shutting_down: AtomicBool::new(false),
            children: Mutex::new(Vec::new()),
            tasks: Mutex::new(Vec::new()),
            data_dir: PathBuf::new(),
            telemetry: telemetry.clone(),
        });
        let supervisor = OutputSupervisor { inner };
        let seq = supervisor.render(LiveFrame::Black);
        assert_eq!(seq, 1, "the supervisor's seq starts at 1, never 0");
        telemetry.note_ack(seq);
        telemetry.finish_session(1_000, SessionOutcome::Clean);
        let row = telemetry.take_buffered_rows().pop().expect("a row");
        assert!(
            row.cue_latency_p95_ms.is_some(),
            "the dispatch→ACK round trip was measured"
        );
    }

    #[test]
    fn a_stale_pidfile_scan_never_panics() {
        // Reads two directories, either of which may be missing or unreadable
        // (a fresh install has no `pidfiles/` yet) — that is a "no", never a
        // panic, on the startup path of every launch.
        let dir = tempfile::tempdir().expect("tempdir");
        let _ = stale_pidfiles_present(dir.path());
        let _ = stale_pidfiles_present(&dir.path().join("never-created"));
    }

    #[test]
    fn binary_override_requires_existing_file() {
        // A bogus override must not be returned (the caller falls back to the
        // in-process windows instead of spawning nothing).
        std::env::set_var("SUNDAYSTAGE_OUTPUT_BIN", "/definitely/not/here");
        assert!(output_binary_path().is_none());
        std::env::remove_var("SUNDAYSTAGE_OUTPUT_BIN");
    }
}
