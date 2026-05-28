//! Repositories — one per aggregate root.
//!
//! Convention:
//!   - Every method is `async fn(&self, ...) -> AppResult<T>`
//!   - Tauri commands NEVER touch sqlx directly — they go through here
//!   - Each repository owns its CRUD + aggregate-specific queries
//!   - Search-related queries live with their primary entity (songs::search)

pub mod arrangements;
pub mod bible;
pub mod decks;
pub mod libraries;
pub mod media;
pub mod services;
pub mod songs;
pub mod themes;

pub use arrangements::ArrangementRepo;
pub use bible::BibleRepo;
pub use decks::DeckRepo;
pub use libraries::LibraryRepo;
pub use media::MediaRepo;
pub use services::ServiceRepo;
pub use songs::SongRepo;
pub use themes::ThemeRepo;
