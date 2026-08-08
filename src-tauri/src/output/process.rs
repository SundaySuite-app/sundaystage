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
//! pidfile next to the socket before respawning, so a relaunch never stacks
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

use crate::output::ipc::{endpoint_path, IpcListener};
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

fn pidfile_path(label: &str) -> PathBuf {
    let mut p = endpoint_path(label);
    p.set_extension("pid");
    p
}

/// Best-effort kill of a stale child left over from a crashed main process,
/// recorded in the pidfile. Never touches anything when the file is absent.
/// Returns whether a stale child was found — E3's `staleChildReaped` signal.
fn reap_stale_child(label: &str) -> bool {
    let pidfile = pidfile_path(label);
    let Ok(s) = std::fs::read_to_string(&pidfile) else {
        return false;
    };
    if let Ok(pid) = s.trim().parse::<u32>() {
        tracing::warn!(label, pid, "reaping a stale output process");
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
    let _ = std::fs::remove_file(&pidfile);
    true
}

/// Whether any output pidfile is still on disk — i.e. a child from a previous
/// process may still be holding its last frame on a projector.
///
/// Read at startup by [`crate::telemetry::quality::reconstruct_previous_session`]
/// to decide the `staleChildReaped` flag on the reconstructed row.
///
/// **Unix only.** On Windows [`endpoint_path`] returns a named-pipe path, so
/// `pidfile_path` produces `\\.\pipe\sundaystage-<label>.pid` — which is not a
/// filesystem location, cannot be written, and therefore cannot be scanned.
/// The pidfile mechanism is inert on Windows today (a pre-existing gap, noted
/// in the E3 report rather than silently worked around here); this function
/// reports `false` there instead of pretending otherwise.
pub fn stale_pidfiles_present() -> bool {
    #[cfg(unix)]
    {
        let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else {
            return false;
        };
        rd.filter_map(Result::ok).any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("sundaystage-") && name.ends_with(".pid")
        })
    }
    #[cfg(not(unix))]
    {
        false
    }
}

impl OutputSupervisor {
    /// Spawn + supervise one child per spec. Must be called on a tokio runtime
    /// (Tauri's async runtime or `#[tokio::test]`).
    pub fn start(
        binary: PathBuf,
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
    if reap_stale_child(&spec.label) {
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
    let _ = std::fs::write(
        pidfile_path(&spec.label),
        child.id().unwrap_or_default().to_string(),
    );

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
                        // Give it a moment, then make sure.
                        let _ = tokio::time::timeout(
                            Duration::from_millis(1_000), child.wait()).await;
                        let _ = child.kill().await;
                        let _ = std::fs::remove_file(pidfile_path(&spec.label));
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

    #[test]
    fn pidfile_path_derives_from_endpoint() {
        let p = pidfile_path("output-main-0");
        assert!(p.to_string_lossy().contains("sundaystage-output-main-0"));
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("pid"));
    }

    #[test]
    fn reaping_without_a_pidfile_is_a_noop_and_reports_nothing_found() {
        // Must never error or kill anything when no stale child exists — and
        // must not raise the `staleChildReaped` quality signal either.
        assert!(!reap_stale_child("output-test-never-spawned"));
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
        // Reads the system temp dir; on Windows the pidfile path is a named
        // pipe and the answer is a flat `false`.
        let _ = stale_pidfiles_present();
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
