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
use crate::output::{OutputAck, OutputMessage};
use crate::services::live_session::LiveFrame;

/// How often the supervisor proves the main process is alive to each child.
pub const HEARTBEAT_MS: u64 = 250;
/// How long a dead child waits before being respawned.
const RESTART_BACKOFF_MS: u64 = 500;
/// How long we wait for a spawned child to connect before retrying.
const CONNECT_TIMEOUT_MS: u64 = 10_000;

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
fn reap_stale_child(label: &str) {
    let pidfile = pidfile_path(label);
    let Ok(s) = std::fs::read_to_string(&pidfile) else {
        return;
    };
    if let Ok(pid) = s.trim().parse::<u32>() {
        tracing::warn!("reaping stale output process {pid} for {label}");
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
}

impl OutputSupervisor {
    /// Spawn + supervise one child per spec. Must be called on a tokio runtime
    /// (Tauri's async runtime or `#[tokio::test]`).
    pub fn start(binary: PathBuf, specs: Vec<OutputSpec>) -> Self {
        let (tx, _) = broadcast::channel::<OutputMessage>(64);
        let inner = Arc::new(Inner {
            tx,
            last_frame: Mutex::new(None),
            seq: AtomicU64::new(0),
            shutting_down: AtomicBool::new(false),
            children: Mutex::new(Vec::new()),
            tasks: Mutex::new(Vec::new()),
        });

        let mut tasks = Vec::new();
        for spec in specs {
            let shared = Arc::new(ChildShared {
                label: spec.label.clone(),
                pid: Mutex::new(None),
                connected: AtomicBool::new(false),
                restarts: AtomicU64::new(0),
                last_acked_seq: AtomicU64::new(0),
            });
            inner.children.lock().push(shared.clone());
            tasks.push(tokio::spawn(supervise_child(
                inner.clone(),
                binary.clone(),
                spec,
                shared,
            )));
        }
        // The heartbeat pump: one timer feeds every child via the broadcast.
        {
            let inner = inner.clone();
            tasks.push(tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(HEARTBEAT_MS));
                loop {
                    tick.tick().await;
                    if inner.shutting_down.load(Ordering::SeqCst) {
                        break;
                    }
                    let _ = inner.tx.send(OutputMessage::Heartbeat {
                        at: crate::db::now_ms(),
                    });
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
        seq
    }

    /// Graceful teardown: tell children to shut down, then reap them.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        let _ = self.inner.tx.send(OutputMessage::Shutdown);
        // Give children a moment to exit cleanly; supervision loops observe
        // `shutting_down` and kill whatever remains.
        tokio::time::sleep(Duration::from_millis(300)).await;
        for t in self.inner.tasks.lock().drain(..) {
            t.abort();
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
    reap_stale_child(&spec.label);

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
                tracing::warn!(
                    "output process {} died — restarting (hold-last-frame covered the gap)",
                    spec.label
                );
            }
            Err(e) => {
                if inner.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                shared.restarts.fetch_add(1, Ordering::SeqCst);
                tracing::error!("output process {} failed: {e} — retrying", spec.label);
            }
        }
        shared.connected.store(false, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(RESTART_BACKOFF_MS)).await;
    }
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
            tracing::warn!("output {} exited before connecting: {status:?}", spec.label);
            return Ok(ChildExit::Died);
        }
        _ = tokio::time::sleep(Duration::from_millis(CONNECT_TIMEOUT_MS)) => {
            tracing::error!("output {} never connected — killing", spec.label);
            let _ = child.kill().await;
            return Ok(ChildExit::Died);
        }
    };
    shared.connected.store(true, Ordering::SeqCst);
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
                }
                Ok(Some(OutputAck::Error { message })) => {
                    // Never a dialog during a service — log; the operator UI
                    // surfaces it as a toast via status polling.
                    tracing::warn!("output {} reported: {message}", spec.label);
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

    #[test]
    fn pidfile_path_derives_from_endpoint() {
        let p = pidfile_path("output-main-0");
        assert!(p.to_string_lossy().contains("sundaystage-output-main-0"));
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("pid"));
    }

    #[test]
    fn reaping_without_a_pidfile_is_a_noop() {
        // Must never error or kill anything when no stale child exists.
        reap_stale_child("output-test-never-spawned");
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
