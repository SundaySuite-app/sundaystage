//! Repositories — one per aggregate root.
//!
//! Convention:
//!   - Every method is `async fn(&self, ...) -> AppResult<T>`
//!   - Tauri commands NEVER touch sqlx directly — they go through here
//!   - Each repository owns its CRUD + aggregate-specific queries
//!   - Search-related queries live with their primary entity (songs::search)

pub mod libraries;
pub mod songs;
pub mod services;
pub mod media;
pub mod bible;
pub mod decks;
pub mod themes;
pub mod arrangements;

pub use libraries::LibraryRepo;
pub use songs::SongRepo;
pub use services::ServiceRepo;
pub use media::MediaRepo;
pub use bible::BibleRepo;
pub use decks::DeckRepo;
pub use themes::ThemeRepo;
pub use arrangements::ArrangementRepo;
