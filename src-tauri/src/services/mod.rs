//! Business logic that doesn't naturally fit a single repository.
//!
//! Currently:
//!   - `cue_list` — Phase 5.1 service compiler that walks a Service +
//!     its items + their songs/scripture/decks and produces a flat
//!     CueList for the live engine to execute.

pub mod bible;
pub mod cue_list;
