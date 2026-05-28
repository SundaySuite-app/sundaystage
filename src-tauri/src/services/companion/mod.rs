//! Phase 12 — the congregation "follow along" companion.
//!
//! [`publisher`] is the desktop side: it turns the live output frame into a
//! tiny, text-only broadcast payload that phones render. The realtime transport
//! (Supabase Realtime channel `companion:{church}:{service}`) rides on the
//! Phase 9 cloud layer and is a documented follow-up; the payload transform
//! here is pure and fully tested, and the schema is the contract the PWA
//! (`companion/`) consumes.

pub mod publisher;
