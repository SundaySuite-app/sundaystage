//! Business logic that doesn't naturally fit a single repository.
//!
//! Currently:
//!   - `cue_list` — Phase 5.1 service compiler that walks a Service +
//!     its items + their songs/scripture/decks and produces a flat
//!     CueList for the live engine to execute.

pub mod ai;
pub mod bible;
pub mod bible_download;
pub mod companion;
pub mod cue_list;
pub mod demo;
pub mod display;
pub mod import_easyworship;
pub mod import_propresenter;
pub mod import_songselect;
pub mod library_publish;
pub mod live_session;
pub mod media;
pub mod rtf;
pub mod scripture_break;
pub mod session_store;
pub mod slide_doc;
pub mod song_export;
pub mod song_import;
pub mod song_usage;
pub mod stage_display;
pub mod sundayplan;
pub mod sundayrec_bridge;
pub mod sync;
pub mod text_fit;
pub mod theme;
pub mod tono;
pub mod update_channel;
