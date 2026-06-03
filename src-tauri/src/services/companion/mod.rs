//! Phase 12 — the congregation "follow along" companion.
//!
//! [`publisher`] is the desktop side: it turns the live output frame into a
//! tiny, text-only broadcast payload that phones render. [`transport`] carries
//! those payloads to phones over a Supabase Realtime channel scoped per service
//! (`companion:{service_id}`), driven on cue advance and service end. Both are
//! pure (the network call sits behind a DI seam, mirroring the sync engine), so
//! sequencing, channel scoping, and the event shape — the contract the PWA
//! (`companion/`) consumes — are fully unit-testable without a network.

pub mod publisher;
pub mod transport;
