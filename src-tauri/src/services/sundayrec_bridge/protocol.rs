//! Phase 10.1 — the SundayStage ↔ SundayRec local bridge protocol.
//!
//! Two apps on the same machine discover each other and share data. This file
//! is the **wire contract** (the design the SundayRec side implements against —
//! see `docs/SUNDAY_BRIDGE_PROTOCOL.md`). It is versioned from day one; the
//! only forward-compatible change is *adding* verbs, never changing existing
//! ones.
//!
//! The transport (HTTP+JSON over loopback, mDNS discovery, two-sided pairing
//! confirmation) is deliberately not implemented here — it needs a live
//! network and the peer app, which can't be exercised headlessly. The message
//! types and version are what's testable now and what both apps must agree on.

use serde::{Deserialize, Serialize};

/// Bumped only by *adding* verbs. Both apps send this in `ping`.
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// A request sent across the bridge. `verb` discriminates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum BridgeRequest {
    /// Capability/liveness check.
    Ping,
    /// SundayRec → Stage: a recording started.
    RecordingStarted {
        recording_id: String,
        started_at: i64,
    },
    /// SundayRec → Stage: a recording stopped.
    RecordingStopped {
        recording_id: String,
        stopped_at: i64,
    },
    /// Stage → SundayRec: a cue advanced; carries a timeline marker.
    CueAdvanced {
        offset_ms: i64,
        title: String,
        cue_index: usize,
    },
    /// Stage → SundayRec: list available recordings.
    GetRecordings,
    /// Stage → SundayRec: fetch a recording's transcript.
    GetTranscript { recording_id: String },
    /// Stage → SundayRec: song-usage history from SundayRec metadata.
    GetSongHistory,
}

/// A response. `result` discriminates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum BridgeResponse {
    Pong {
        app: String,
        version: String,
        capabilities: Vec<String>,
    },
    Ok,
    Recordings {
        recordings: Vec<RecordingRef>,
    },
    Transcript {
        recording_id: String,
        text: String,
    },
    SongHistory {
        songs: Vec<SongHistoryEntry>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingRef {
    pub id: String,
    pub title: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongHistoryEntry {
    pub song_id: String,
    pub title: String,
    pub last_used_at: i64,
}

/// This app's `Pong` — what SundayStage answers a `ping` with.
pub fn this_app_pong() -> BridgeResponse {
    BridgeResponse::Pong {
        app: "sundaystage".to_string(),
        version: PROTOCOL_VERSION.to_string(),
        capabilities: vec![
            "cue_advanced".into(),
            "export_srt".into(),
            "chapter_markers".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_with_snake_case_verbs() {
        let cases = [
            BridgeRequest::Ping,
            BridgeRequest::RecordingStarted {
                recording_id: "r1".into(),
                started_at: 100,
            },
            BridgeRequest::CueAdvanced {
                offset_ms: 12_500,
                title: "Amazing Grace".into(),
                cue_index: 3,
            },
            BridgeRequest::GetTranscript {
                recording_id: "r1".into(),
            },
        ];
        for req in cases {
            let json = serde_json::to_string(&req).unwrap();
            assert_eq!(serde_json::from_str::<BridgeRequest>(&json).unwrap(), req);
        }
        // Verb tag is the stable wire key.
        let j = serde_json::to_string(&BridgeRequest::GetSongHistory).unwrap();
        assert!(j.contains("\"verb\":\"get_song_history\""));
    }

    #[test]
    fn responses_round_trip() {
        let cases = [
            this_app_pong(),
            BridgeResponse::Ok,
            BridgeResponse::Recordings {
                recordings: vec![RecordingRef {
                    id: "r".into(),
                    title: "Sunday".into(),
                    started_at: 1,
                }],
            },
            BridgeResponse::Error {
                message: "nope".into(),
            },
        ];
        for resp in cases {
            let json = serde_json::to_string(&resp).unwrap();
            assert_eq!(serde_json::from_str::<BridgeResponse>(&json).unwrap(), resp);
        }
    }

    #[test]
    fn pong_advertises_version_and_capabilities() {
        match this_app_pong() {
            BridgeResponse::Pong {
                app,
                version,
                capabilities,
            } => {
                assert_eq!(app, "sundaystage");
                assert_eq!(version, PROTOCOL_VERSION);
                assert!(capabilities.contains(&"export_srt".to_string()));
            }
            _ => panic!("expected pong"),
        }
    }
}
