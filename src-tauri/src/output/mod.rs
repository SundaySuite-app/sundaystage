//! Phase 5.2 — output-process protocol + watchdog.
//!
//! The most important reliability decision in the app: the live output runs in
//! a **separate OS process** (`sundaystage-output`) so that if the main UI
//! crashes, the projector keeps showing the current slide. This module is the
//! shared contract between the two processes — the wire protocol and the
//! watchdog — kept as pure, fully-tested logic.
//!
//! ## Module map
//!
//! * here — the message protocol ([`OutputMessage`] / [`OutputAck`]) and the
//!   [`Watchdog`] that decides "hold the last frame" when the main app goes
//!   quiet;
//! * [`ipc`] — the local-IPC transport (Unix socket / Windows named pipe,
//!   newline-delimited JSON) the protocol travels over;
//! * [`process`] — spawning + supervising one `sundaystage-output` process
//!   per display (restart on crash, current-frame resend, pidfile reaping);
//! * [`window`] — the legacy in-process webview windows, kept as the
//!   `process_isolation: false` fallback and as the dev-mode fallback when
//!   the output binary isn't built.
//!
//! The end-to-end spawn → handshake → render/ack → crash/restart story is
//! verified headlessly against the real binary in `tests/output_isolation.rs`;
//! only real pixels/full-screen placement need a screen (rig test).

use serde::{Deserialize, Serialize};

use crate::services::live_session::LiveFrame;

pub mod ipc;
pub mod process;
pub mod window;

/// Main app → output process. `seq` lets the output ACK the exact render and
/// lets us measure keypress→pixels latency (the < 50 ms promise).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputMessage {
    /// Render this frame now.
    Render { frame: LiveFrame, seq: u64 },
    /// Liveness ping; the output's watchdog resets on each one.
    Heartbeat { at: i64 },
    /// Graceful teardown (end of service).
    Shutdown,
}

/// Output process → main app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputAck {
    /// The frame for `seq` is on screen.
    Rendered { seq: u64, rendered_at: i64 },
    /// Something went wrong rendering; the main app can surface it as a
    /// non-blocking toast (never a dialog during a service).
    Error { message: String },
}

/// How long without a heartbeat before the link is considered dead. The output
/// keeps the last frame visible past this — it never blanks the congregation.
pub const DEFAULT_TIMEOUT_MS: i64 = 2000;

/// Tracks the last heartbeat and decides whether the main app is still alive.
/// Pure: feed it timestamps, ask it questions.
#[derive(Debug, Clone)]
pub struct Watchdog {
    last_beat: i64,
    timeout_ms: i64,
}

impl Watchdog {
    pub fn new(now: i64) -> Self {
        Self {
            last_beat: now,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    pub fn with_timeout(now: i64, timeout_ms: i64) -> Self {
        Self {
            last_beat: now,
            timeout_ms,
        }
    }

    /// Record a heartbeat from the main app.
    pub fn beat(&mut self, now: i64) {
        self.last_beat = now;
    }

    /// Is the main app still considered connected at `now`?
    pub fn is_alive(&self, now: i64) -> bool {
        now.saturating_sub(self.last_beat) <= self.timeout_ms
    }

    /// The single decision the output process acts on: when the link is dead,
    /// **hold the last frame** (do not blank).
    pub fn should_hold_last_frame(&self, now: i64) -> bool {
        !self.is_alive(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_keeps_link_alive_then_times_out() {
        let mut wd = Watchdog::with_timeout(1_000, 2_000);
        assert!(wd.is_alive(1_500)); // within window
        assert!(wd.is_alive(3_000)); // exactly at timeout boundary
        assert!(!wd.is_alive(3_001)); // past it → dead
                                      // A fresh beat revives it.
        wd.beat(3_001);
        assert!(wd.is_alive(4_000));
    }

    #[test]
    fn dead_link_holds_last_frame() {
        let wd = Watchdog::with_timeout(0, 2_000);
        assert!(!wd.should_hold_last_frame(1_000)); // alive → keep rendering normally
        assert!(wd.should_hold_last_frame(5_000)); // dead → hold last frame
    }

    #[test]
    fn render_message_round_trips_with_frame() {
        let msg = OutputMessage::Render {
            frame: LiveFrame::Black,
            seq: 7,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: OutputMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
        // Tag is snake_case for a stable wire format.
        assert!(json.contains("\"type\":\"render\""));
        assert!(json.contains("\"seq\":7"));
    }

    #[test]
    fn heartbeat_and_shutdown_round_trip() {
        for msg in [
            OutputMessage::Heartbeat { at: 123 },
            OutputMessage::Shutdown,
        ] {
            let json = serde_json::to_string(&msg).unwrap();
            let back: OutputMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(msg, back);
        }
    }

    #[test]
    fn ack_round_trips() {
        let ack = OutputAck::Rendered {
            seq: 7,
            rendered_at: 999,
        };
        let json = serde_json::to_string(&ack).unwrap();
        assert_eq!(serde_json::from_str::<OutputAck>(&json).unwrap(), ack);
        let err = OutputAck::Error {
            message: "gpu lost".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(serde_json::from_str::<OutputAck>(&json).unwrap(), err);
    }
}
