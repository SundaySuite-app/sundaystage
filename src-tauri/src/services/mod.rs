//! Business logic that doesn't naturally fit a single repository.
//!
//! Currently:
//!   - `cue_list` — Phase 5.1 service compiler that walks a Service +
//!     its items + their songs/scripture/decks and produces a flat
//!     CueList for the live engine to execute.

pub mod ai;
pub mod bible;
pub mod cue_list;
pub mod demo;
pub mod media;
pub mod live_session;
pub mod session_store;
pub mod slide_doc;
pub mod stage_display;
pub mod sundayrec_bridge;
pub mod sync;
pub mod theme;
