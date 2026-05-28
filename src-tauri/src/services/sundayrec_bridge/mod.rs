//! Phase 10 — the SundayRec integration (the unique Sunday-suite feature).
//!
//! [`protocol`] defines the versioned local bridge contract; [`export`] turns
//! a finished live session into recording chapter markers and an SRT caption
//! file. The loopback transport, mDNS discovery, two-sided pairing, and the
//! TONO streaming-licence audit are documented follow-ups (see
//! `docs/SUNDAY_BRIDGE_PROTOCOL.md`) — they need a live network and the peer
//! app, which can't be exercised headlessly.

pub mod export;
pub mod protocol;
