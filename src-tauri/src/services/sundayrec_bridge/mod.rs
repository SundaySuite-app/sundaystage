//! Phase 10 — the SundayRec integration (the unique Sunday-suite feature).
//!
//! [`protocol`] defines the versioned local bridge contract; [`dispatch`]
//! answers inbound peer requests (pure); [`export`] turns a finished live
//! session into recording chapter markers and an SRT caption file; [`manifest`]
//! joins the session back to the service plan into the `service-manifest.json`
//! SundayRec imports (setlist + chapters with CCLI/TONO ids). The TONO
//! streaming-licence logic lives in [`crate::services::tono`].
//!
//! The loopback transport, mDNS discovery, and two-sided pairing are documented
//! follow-ups (see `docs/SUNDAY_BRIDGE_PROTOCOL.md`) — they need a live network
//! and the peer app, which can't be exercised headlessly.

pub mod dispatch;
pub mod export;
pub mod manifest;
pub mod protocol;
