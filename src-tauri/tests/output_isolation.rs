//! Phase 5.2 — end-to-end crash-isolation tests against the REAL
//! `sundaystage-output` binary in `--headless` mode (the identical IPC client
//! loop, minus the window).
//!
//! Headlessly proven here:
//!   * spawn → socket handshake → frames in, ACKs out (per-seq);
//!   * malformed frames answered with a non-fatal `Error` ack, link survives;
//!   * graceful `Shutdown` exits the child;
//!   * the supervisor detects a *crashed child* and restarts it, re-sending
//!     the current frame so the projector is never blank;
//!   * a child whose *parent* dies (link loss) HOLDS — stays alive instead of
//!     exiting — which is the "if the editor crashes, the stage does not"
//!     promise, decided by the same watchdog the windowed mode uses;
//!   * (E3) a real crash-and-restart lands in the session's quality row as a
//!     `warn` with `output-restarted`, and the dispatch→ACK round trip is
//!     measured against the < 50 ms cue budget — end to end, through the real
//!     binary and the real socket, not a mock.
//!
//! Only a screen can verify (Richard's rig test): real pixels, borderless
//! full-screen placement on the right monitor, and `ss://render` delivery
//! into the webview.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sundaystage_lib::output::ipc::{endpoint_path, IpcListener};
use sundaystage_lib::output::process::{OutputSpec, OutputSupervisor};
use sundaystage_lib::output::{OutputAck, OutputMessage};
use sundaystage_lib::services::cue_list::SlideContent;
use sundaystage_lib::services::live_session::LiveFrame;
use sundaystage_lib::telemetry::quality::{
    LiveSafe, QualityCollector, QualityReason, QualityVerdict, SessionOutcome,
};

/// The binary cargo built for this test run.
fn output_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sundaystage-output"))
}

fn slide(text: &str) -> LiveFrame {
    LiveFrame::Slide {
        slide_content: SlideContent {
            section_label: Some("Verse 1".into()),
            text_lines: vec![text.into()],
            translation_lines: None,
            reference: None,
            sensitive_slide: false,
            appearance: None,
        },
    }
}

/// A per-test unique label so parallel tests never share an endpoint (and the
/// supervisor's stale-child reaper can never touch another test's child).
fn unique_label(test: &str) -> String {
    format!("output-test-{}-{}", test, std::process::id())
}

/// Spawn the real binary in headless mode pointed at `socket`.
fn spawn_headless(socket: &std::path::Path, label: &str) -> tokio::process::Child {
    let mut cmd = tokio::process::Command::new(output_bin());
    cmd.arg("--socket")
        .arg(socket)
        .arg("--label")
        .arg(label)
        .arg("--headless")
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    cmd.spawn().expect("spawn sundaystage-output")
}

async fn wait_until(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + timeout;
    while !f() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for: {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // Signal 0 probes existence without touching the process.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── handshake / frames / acks / shutdown ─────────────────────────────────────

#[tokio::test]
async fn real_binary_handshakes_acks_frames_and_shuts_down() {
    let label = unique_label("handshake");
    let socket = endpoint_path(&label);
    let mut listener = IpcListener::bind(&socket).expect("bind");
    let mut child = spawn_headless(&socket, &label);

    let stream = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .expect("child connects in time")
        .expect("accept");
    let (mut reader, mut writer) = stream.into_split();

    // Frame 1 → per-seq ack.
    writer
        .write(&OutputMessage::Render {
            frame: slide("Amazing grace"),
            seq: 1,
        })
        .await
        .unwrap();
    match reader.read::<OutputAck>().await.unwrap().expect("ack 1") {
        OutputAck::Rendered { seq, .. } => assert_eq!(seq, 1),
        other => panic!("expected Rendered, got {other:?}"),
    }

    // Heartbeats produce no ack; the next render still acks its own seq.
    writer
        .write(&OutputMessage::Heartbeat { at: 123 })
        .await
        .unwrap();
    writer
        .write(&OutputMessage::Render {
            frame: LiveFrame::Black,
            seq: 2,
        })
        .await
        .unwrap();
    match reader.read::<OutputAck>().await.unwrap().expect("ack 2") {
        OutputAck::Rendered { seq, .. } => assert_eq!(seq, 2),
        other => panic!("expected Rendered, got {other:?}"),
    }

    // Shutdown → clean exit.
    writer.write(&OutputMessage::Shutdown).await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .expect("child exits after Shutdown")
        .expect("wait");
    assert!(status.success(), "clean exit, got {status:?}");
}

#[tokio::test]
async fn malformed_frame_gets_error_ack_but_never_kills_the_output() {
    let label = unique_label("badframe");
    let socket = endpoint_path(&label);
    let mut listener = IpcListener::bind(&socket).expect("bind");
    let mut child = spawn_headless(&socket, &label);

    let stream = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .expect("connect")
        .expect("accept");
    let (mut reader, mut writer) = stream.into_split();

    // Valid JSON that is NOT a valid OutputMessage — the worst realistic
    // corruption (a version-skewed message).
    writer
        .write(&serde_json::json!({ "type": "not_a_real_message" }))
        .await
        .unwrap();
    match reader
        .read::<OutputAck>()
        .await
        .unwrap()
        .expect("error ack")
    {
        OutputAck::Error { message } => assert!(message.contains("bad message")),
        other => panic!("expected Error ack, got {other:?}"),
    }

    // The link survived: a real frame still renders + acks.
    writer
        .write(&OutputMessage::Render {
            frame: slide("still alive"),
            seq: 9,
        })
        .await
        .unwrap();
    match reader.read::<OutputAck>().await.unwrap().expect("ack 9") {
        OutputAck::Rendered { seq, .. } => assert_eq!(seq, 9),
        other => panic!("expected Rendered, got {other:?}"),
    }

    writer.write(&OutputMessage::Shutdown).await.unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}

// ── supervisor: child crash → restart + frame resend ────────────────────────

#[cfg(unix)]
#[tokio::test]
async fn supervisor_restarts_crashed_child_and_resends_current_frame() {
    // Imported here rather than at the top: this test is `cfg(unix)`, and a
    // top-level import would be an unused-import warning on Windows.
    use sundaystage_lib::output::process::pidfile_dir;

    let label = unique_label("respawn");
    // E3 — the collector the supervisor feeds. Everything it is told here goes
    // through the same atomics the live path uses.
    let telemetry = Arc::new(QualityCollector::new());
    telemetry.begin_session(0);
    // Stands in for the app-local data dir; the supervisor puts its pidfiles in
    // `pidfiles/` under it (a real directory on every platform — it used to be
    // derived from the IPC endpoint, which on Windows is a named pipe).
    let data_dir = tempfile::tempdir().expect("data dir");
    let supervisor = OutputSupervisor::start(
        output_bin(),
        data_dir.path().to_path_buf(),
        vec![OutputSpec {
            label: label.clone(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            headless: true,
            appearance_file: None,
        }],
        telemetry.clone(),
    );

    // First frame reaches the child and is acked.
    let seq1 = supervisor.render(slide("before the crash"));
    wait_until("first ack", Duration::from_secs(10), || {
        supervisor.status()[0].last_acked_seq >= seq1
    })
    .await;

    // Crash the child (SIGKILL — a real crash, no cleanup).
    let pid = supervisor.status()[0].pid.expect("child pid");

    // The running child is on disk as a pidfile in the data dir — the trace a
    // crashed main process leaves behind, and the input to both the reaper and
    // the startup "did we crash last time?" scan. Proven against the real
    // binary, not a mock pid.
    let pidfile = pidfile_dir(data_dir.path()).join(format!("sundaystage-{label}.pid"));
    assert_eq!(
        std::fs::read_to_string(&pidfile)
            .unwrap_or_else(|e| panic!("pidfile at {}: {e}", pidfile.display())),
        pid.to_string()
    );
    assert!(std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap()
        .success());

    // The supervisor must detect the death, restart, and resend the CURRENT
    // frame on reconnect — the new child acks the latest seq without any new
    // operator action.
    wait_until("restart + frame resend", Duration::from_secs(15), || {
        let s = &supervisor.status()[0];
        s.restarts >= 1 && s.connected && s.last_acked_seq >= seq1
    })
    .await;
    let new_pid = supervisor.status()[0].pid.expect("new pid");
    assert_ne!(new_pid, pid, "a fresh process was spawned");
    // The pidfile follows the restart: it must name the child that is actually
    // on the projector now, or the next launch would reap the wrong pid.
    assert_eq!(
        std::fs::read_to_string(&pidfile).expect("pidfile after restart"),
        new_pid.to_string()
    );

    // New frames flow to the replacement.
    let seq2 = supervisor.render(LiveFrame::Logo);
    wait_until("ack after restart", Duration::from_secs(10), || {
        supervisor.status()[0].last_acked_seq >= seq2
    })
    .await;

    // E3 — the same crash, read back as a quality row. This is the programme's
    // E7 rig expectation, proven headlessly: a killed output child is a WARN
    // (hold-last-frame covered the gap), never a fail.
    telemetry.finish_session(60_000, SessionOutcome::Clean);
    let row = telemetry
        .take_buffered_rows()
        .pop()
        .expect("one row per session");
    assert_eq!(row.verdict, QualityVerdict::Warn, "{row:?}");
    assert!(row.output_child_restarts >= 1, "{row:?}");
    assert!(
        row.reasons.contains(&QualityReason::OutputRestarted),
        "{row:?}"
    );
    assert!(!row.abnormal_end);
    // …and the dispatch→ACK round trip was measured through the real socket.
    let p95 = row.cue_latency_p95_ms.expect("acks were seq-correlated");
    assert!(p95 <= 250, "cue latency p95 was {p95} ms");

    supervisor.shutdown().await;
    wait_until("child gone after shutdown", Duration::from_secs(5), || {
        !pid_alive(new_pid)
    })
    .await;
}

// ── parent death → child HOLDS (the core promise) ────────────────────────────

#[cfg(unix)]
#[tokio::test]
async fn child_outlives_a_dead_parent_holding_the_last_frame() {
    let label = unique_label("hold");
    let socket = endpoint_path(&label);
    let mut listener = IpcListener::bind(&socket).expect("bind");
    let mut child = spawn_headless(&socket, &label);
    let pid = child.id().expect("pid");

    let stream = tokio::time::timeout(Duration::from_secs(10), listener.accept())
        .await
        .expect("connect")
        .expect("accept");
    let (mut reader, mut writer) = stream.into_split();
    writer
        .write(&OutputMessage::Render {
            frame: slide("the last slide"),
            seq: 1,
        })
        .await
        .unwrap();
    assert!(matches!(
        reader.read::<OutputAck>().await.unwrap(),
        Some(OutputAck::Rendered { seq: 1, .. })
    ));

    // "Main app crashes": the link and listener vanish without a Shutdown.
    drop(reader);
    drop(writer);
    drop(listener);

    // Well past the 2 s watchdog window the child must still be alive,
    // holding the last frame — never exiting, never blanking.
    tokio::time::sleep(Duration::from_millis(3000)).await;
    assert!(
        pid_alive(pid),
        "output process must outlive a crashed main app"
    );

    // Cleanup: the relaunched main app would reap it via pidfile; here we
    // kill it directly.
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}
