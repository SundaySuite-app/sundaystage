//! Phase 12.2 — companion broadcast transport.
//!
//! The [`publisher`](super::publisher) reduces a live frame to a tiny,
//! text-only [`BroadcastFrame`]. This module is the missing piece: the
//! **transport** that carries those frames to congregation phones over a
//! Supabase Realtime channel, scoped per service so two parallel services in
//! the same church never cross-talk.
//!
//! Like the rest of the cloud layer (see [`crate::services::sync`]), the actual
//! network/Supabase call is gated behind a trait — a DI seam — so the
//! sequencing, channel scoping, and event shape are pure and fully
//! unit-testable without a network or an account this environment can't
//! provide. The real Supabase Realtime client rides on the same Phase 9 cloud
//! backbone and is a documented follow-up ([`RealtimeTransport`]).
//!
//! Flow: the live engine calls [`CompanionBroadcaster::on_cue_advance`] after
//! every cue advance and [`CompanionBroadcaster::on_service_end`] when the
//! service ends; the broadcaster assigns the monotonic `seq`, transforms the
//! frame, and hands a [`BroadcastEvent`] to the transport.

use serde::{Deserialize, Serialize};

use crate::services::companion::publisher::{to_broadcast, BroadcastFrame};
use crate::services::live_session::LiveFrame;

/// The Realtime channel a phone subscribes to. Scoped by `service_id` so each
/// running service is isolated; the topic string is the contract the PWA joins.
pub fn channel_for(service_id: &str) -> String {
    format!("companion:{service_id}")
}

/// What the desktop publishes to the channel. The PWA pattern-matches on
/// `event`; `frame` is present on every advance, absent on service end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BroadcastEvent {
    /// The operator advanced to a new cue — render this frame.
    CueAdvance { frame: BroadcastFrame },
    /// The service ended — phones should show the "service over" state.
    ServiceEnd,
}

/// The transport seam. A real implementation publishes to Supabase Realtime; a
/// recording one captures calls for tests. Errors are reported as a string so
/// a failed broadcast can be logged best-effort without taking down the show
/// (the companion is never on the critical live path).
pub trait BroadcastTransport: Send {
    fn publish(&mut self, channel: &str, event: &BroadcastEvent) -> Result<(), String>;
}

/// The production transport. Wired to a real Supabase Realtime client when the
/// Phase 9 cloud backbone lands; until then it is a no-op so wiring the
/// broadcaster into the live path never blocks going live in the free tier.
#[derive(Debug, Default)]
pub struct RealtimeTransport {
    /// The Supabase project URL + anon key would live here once configured.
    /// `None` = cloud not configured (free tier) → publish is a no-op.
    configured: bool,
}

impl RealtimeTransport {
    /// A transport with no cloud configured (free tier). Publishing is a no-op.
    pub fn local_only() -> Self {
        Self { configured: false }
    }
}

impl BroadcastTransport for RealtimeTransport {
    fn publish(&mut self, _channel: &str, _event: &BroadcastEvent) -> Result<(), String> {
        if !self.configured {
            // No cloud configured — silently drop. The live output is unaffected.
            return Ok(());
        }
        // TODO(phase-9): POST to Supabase Realtime
        // `{project}/realtime/v1/api/broadcast` with the channel topic and the
        // serialized `event`, reusing the sync-engine's Supabase credentials.
        Ok(())
    }
}

/// Drives broadcasts for one live session: owns the channel and the monotonic
/// `seq` counter, transforms each frame, and forwards events to the transport.
pub struct CompanionBroadcaster<T: BroadcastTransport> {
    channel: String,
    transport: T,
    seq: u32,
}

impl<T: BroadcastTransport> CompanionBroadcaster<T> {
    pub fn new(service_id: &str, transport: T) -> Self {
        Self::resuming(service_id, transport, 0)
    }

    /// Like [`new`](Self::new) but seeds the monotonic `seq` at `start_seq`.
    ///
    /// This is the crash/restart-safety seam. A phone drops any frame whose
    /// `seq <= lastSeq` it has already seen (see `companion/app.js`). If a new
    /// broadcaster for an already-subscribed service restarted at `seq: 0`
    /// (after a UI crash + `live_recover`, or a second `live_start` re-using a
    /// `service_id`), every post-restart frame would be `<=` the phone's stored
    /// `lastSeq` and silently ignored — freezing the phone on the pre-crash
    /// slide. Seeding above the previous session's last emitted `seq` keeps the
    /// stream monotonic across the restart so phones resync.
    pub fn resuming(service_id: &str, transport: T, start_seq: u32) -> Self {
        Self {
            channel: channel_for(service_id),
            transport,
            seq: start_seq,
        }
    }

    /// The channel topic the PWA must join.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// The next `seq` this broadcaster will assign. Lets the caller continue the
    /// monotonic stream when replacing one broadcaster with another for the same
    /// service (e.g. a restart that re-uses the `service_id`).
    pub fn next_seq(&self) -> u32 {
        self.seq
    }

    /// Publish the current frame after a cue advance. `sensitive` force-gates
    /// the broadcast to a placeholder (in addition to a slide's own
    /// `sensitive_slide` flag). Returns the assigned `seq`.
    pub fn on_cue_advance(&mut self, frame: &LiveFrame, sensitive: bool) -> Result<u32, String> {
        let seq = self.seq;
        self.seq += 1;
        let payload = to_broadcast(frame, seq, sensitive);
        self.transport.publish(
            &self.channel,
            &BroadcastEvent::CueAdvance { frame: payload },
        )?;
        Ok(seq)
    }

    /// Tell phones the service has ended.
    pub fn on_service_end(&mut self) -> Result<(), String> {
        self.transport
            .publish(&self.channel, &BroadcastEvent::ServiceEnd)
    }

    /// Borrow the underlying transport (e.g. to inspect a recording mock).
    pub fn transport(&self) -> &T {
        &self.transport
    }
}

/// A recording transport for tests: captures every `(channel, event)` so a test
/// can assert the cue_advance → publish shape without a network.
#[derive(Debug, Default)]
pub struct RecordingTransport {
    pub published: Vec<(String, BroadcastEvent)>,
    /// When set, the next `publish` fails with this message (to exercise the
    /// best-effort error path).
    pub fail_next: Option<String>,
}

impl BroadcastTransport for RecordingTransport {
    fn publish(&mut self, channel: &str, event: &BroadcastEvent) -> Result<(), String> {
        if let Some(msg) = self.fail_next.take() {
            return Err(msg);
        }
        self.published.push((channel.to_string(), event.clone()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::companion::publisher::BroadcastKind;
    use crate::services::cue_list::SlideContent;

    fn lyric_frame(line: &str) -> LiveFrame {
        LiveFrame::Slide {
            slide_content: SlideContent {
                section_label: Some("Verse 1".into()),
                text_lines: vec![line.to_string()],
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
            },
        }
    }

    fn sensitive_frame(line: &str) -> LiveFrame {
        LiveFrame::Slide {
            slide_content: SlideContent {
                section_label: None,
                text_lines: vec![line.to_string()],
                translation_lines: None,
                reference: None,
                sensitive_slide: true,
            },
        }
    }

    #[test]
    fn channel_is_scoped_by_service() {
        assert_eq!(channel_for("svc-7"), "companion:svc-7");
    }

    #[test]
    fn cue_advance_publishes_to_scoped_channel_with_payload() {
        let mut b = CompanionBroadcaster::new("svc-1", RecordingTransport::default());
        let seq = b.on_cue_advance(&lyric_frame("Holy"), false).unwrap();
        assert_eq!(seq, 0);

        let published = &b.transport().published;
        assert_eq!(published.len(), 1);
        let (channel, event) = &published[0];
        assert_eq!(channel, "companion:svc-1");
        match event {
            BroadcastEvent::CueAdvance { frame } => {
                assert_eq!(frame.kind, BroadcastKind::Lyric);
                assert_eq!(frame.text, "Holy");
                assert_eq!(frame.seq, 0);
            }
            other => panic!("expected cue_advance, got {other:?}"),
        }
    }

    #[test]
    fn seq_is_monotonic_across_advances() {
        let mut b = CompanionBroadcaster::new("svc", RecordingTransport::default());
        assert_eq!(b.on_cue_advance(&lyric_frame("a"), false).unwrap(), 0);
        assert_eq!(b.on_cue_advance(&lyric_frame("b"), false).unwrap(), 1);
        assert_eq!(b.on_cue_advance(&lyric_frame("c"), false).unwrap(), 2);
        let seqs: Vec<u32> = b
            .transport()
            .published
            .iter()
            .map(|(_, e)| match e {
                BroadcastEvent::CueAdvance { frame } => frame.seq,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    #[test]
    fn sensitive_slide_publishes_placeholder_not_content() {
        let mut b = CompanionBroadcaster::new("svc", RecordingTransport::default());
        b.on_cue_advance(&sensitive_frame("Communion liturgy"), false)
            .unwrap();
        match &b.transport().published[0].1 {
            BroadcastEvent::CueAdvance { frame } => {
                assert!(!frame.text.contains("Communion"));
                assert_eq!(frame.text, "Tjeneste pågår");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn caller_force_gate_also_collapses() {
        let mut b = CompanionBroadcaster::new("svc", RecordingTransport::default());
        b.on_cue_advance(&lyric_frame("private"), true).unwrap();
        match &b.transport().published[0].1 {
            BroadcastEvent::CueAdvance { frame } => assert_eq!(frame.text, "Tjeneste pågår"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn service_end_publishes_terminal_event() {
        let mut b = CompanionBroadcaster::new("svc", RecordingTransport::default());
        b.on_cue_advance(&lyric_frame("x"), false).unwrap();
        b.on_service_end().unwrap();
        assert_eq!(b.transport().published.len(), 2);
        assert_eq!(b.transport().published[1].1, BroadcastEvent::ServiceEnd);
    }

    #[test]
    fn blackout_frame_broadcasts_blackout_kind() {
        let mut b = CompanionBroadcaster::new("svc", RecordingTransport::default());
        b.on_cue_advance(&LiveFrame::Black, false).unwrap();
        match &b.transport().published[0].1 {
            BroadcastEvent::CueAdvance { frame } => {
                assert_eq!(frame.kind, BroadcastKind::Blackout);
                assert!(frame.text.is_empty());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn event_round_trips_through_json() {
        // The PWA consumes the JSON shape — guard the serde contract.
        let ev = BroadcastEvent::CueAdvance {
            frame: to_broadcast(&lyric_frame("Holy"), 5, false),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"event\":\"cue_advance\""));
        let back: BroadcastEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ev);

        let end = serde_json::to_string(&BroadcastEvent::ServiceEnd).unwrap();
        assert_eq!(end, "{\"event\":\"service_end\"}");
    }

    #[test]
    fn publish_failure_surfaces_for_best_effort_logging() {
        let transport = RecordingTransport {
            fail_next: Some("offline".into()),
            ..Default::default()
        };
        let mut b = CompanionBroadcaster::new("svc", transport);
        let err = b.on_cue_advance(&lyric_frame("x"), false).unwrap_err();
        assert_eq!(err, "offline");
        // seq still advanced so a retry doesn't reuse it.
        assert_eq!(b.on_cue_advance(&lyric_frame("y"), false).unwrap(), 1);
    }

    #[test]
    fn resuming_seeds_seq_so_restart_stays_above_a_phones_last_seq() {
        // Regression: a phone reached lastSeq=40 in the first session. After a
        // crash + recover, a fresh broadcaster restarted at seq 0 would emit
        // 0,1,2… which the phone's `seq <= lastSeq` guard discards, freezing it.
        // Seeding above the prior last seq keeps the stream monotonic.
        let mut b = CompanionBroadcaster::resuming("svc", RecordingTransport::default(), 41);
        let first = b
            .on_cue_advance(&lyric_frame("after recover"), false)
            .unwrap();
        assert!(
            first > 40,
            "restarted broadcaster must emit seq above the phone's lastSeq (got {first})"
        );
        // …and remains monotonic from there.
        assert_eq!(b.on_cue_advance(&lyric_frame("next"), false).unwrap(), 42);
    }

    #[test]
    fn next_seq_lets_a_replacement_continue_the_stream() {
        let mut a = CompanionBroadcaster::new("svc", RecordingTransport::default());
        a.on_cue_advance(&lyric_frame("a"), false).unwrap(); // seq 0
        a.on_cue_advance(&lyric_frame("b"), false).unwrap(); // seq 1
        assert_eq!(a.next_seq(), 2);
        // Replacing for the same service continues, never resets.
        let mut b =
            CompanionBroadcaster::resuming("svc", RecordingTransport::default(), a.next_seq());
        assert_eq!(b.on_cue_advance(&lyric_frame("c"), false).unwrap(), 2);
    }

    #[test]
    fn realtime_transport_local_only_is_noop_ok() {
        let mut t = RealtimeTransport::local_only();
        assert!(t
            .publish("companion:svc", &BroadcastEvent::ServiceEnd)
            .is_ok());
    }
}
