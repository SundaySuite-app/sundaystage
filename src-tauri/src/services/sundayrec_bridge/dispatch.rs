//! Phase 10.1 — inbound bridge request handling (pure).
//!
//! SundayStage mostly *sends* requests to SundayRec, but it also *answers* a
//! few: `ping`, and the recording/streaming notifications SundayRec pushes.
//! [`BridgeState::handle`] is the pure dispatcher the (deferred) loopback
//! transport will call per inbound request. The query verbs are outbound-from-
//! Stage, so receiving them is reported as unsupported.

use super::protocol::{this_app_pong, BridgeRequest, BridgeResponse};

/// What SundayStage knows about the peer's current state, updated as
/// notifications arrive. The TONO flag (Phase 10.2) reads `streaming`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BridgeState {
    /// Active recording id, if SundayRec is recording.
    pub recording: Option<String>,
    /// Whether SundayRec is currently *streaming* (not just recording).
    pub streaming: bool,
    /// When streaming started (unix ms), if active.
    pub streaming_since: Option<i64>,
}

impl BridgeState {
    /// Handle one inbound request from the peer, mutating tracked state.
    pub fn handle(&mut self, req: BridgeRequest) -> BridgeResponse {
        match req {
            BridgeRequest::Ping => this_app_pong(),
            BridgeRequest::RecordingStarted { recording_id, .. } => {
                self.recording = Some(recording_id);
                BridgeResponse::Ok
            }
            BridgeRequest::RecordingStopped { .. } => {
                self.recording = None;
                BridgeResponse::Ok
            }
            BridgeRequest::StreamingStarted { at } => {
                self.streaming = true;
                self.streaming_since = Some(at);
                BridgeResponse::Ok
            }
            BridgeRequest::StreamingStopped { .. } => {
                self.streaming = false;
                self.streaming_since = None;
                BridgeResponse::Ok
            }
            // Stage sends these to SundayRec; receiving them is unsupported.
            BridgeRequest::CueAdvanced { .. }
            | BridgeRequest::GetRecordings
            | BridgeRequest::GetTranscript { .. }
            | BridgeRequest::GetSongHistory => BridgeResponse::Error {
                message: "verb is outbound-only for sundaystage".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_answers_pong() {
        let mut st = BridgeState::default();
        assert!(matches!(
            st.handle(BridgeRequest::Ping),
            BridgeResponse::Pong { .. }
        ));
    }

    #[test]
    fn recording_notifications_track_state() {
        let mut st = BridgeState::default();
        st.handle(BridgeRequest::RecordingStarted {
            recording_id: "r1".into(),
            started_at: 10,
        });
        assert_eq!(st.recording.as_deref(), Some("r1"));
        st.handle(BridgeRequest::RecordingStopped {
            recording_id: "r1".into(),
            stopped_at: 20,
        });
        assert_eq!(st.recording, None);
    }

    #[test]
    fn streaming_notifications_drive_the_flag() {
        let mut st = BridgeState::default();
        assert!(!st.streaming);
        st.handle(BridgeRequest::StreamingStarted { at: 5 });
        assert!(st.streaming);
        assert_eq!(st.streaming_since, Some(5));
        st.handle(BridgeRequest::StreamingStopped { at: 9 });
        assert!(!st.streaming);
        assert_eq!(st.streaming_since, None);
    }

    #[test]
    fn outbound_verbs_are_unsupported_inbound() {
        let mut st = BridgeState::default();
        assert!(matches!(
            st.handle(BridgeRequest::GetRecordings),
            BridgeResponse::Error { .. }
        ));
    }
}
