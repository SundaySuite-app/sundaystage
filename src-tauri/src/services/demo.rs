//! Phase 13.1 — first-run demo content + supported locales.
//!
//! On first launch the onboarding flow can prefill a library with a small,
//! ready-to-play "Velkomstgudstjeneste" so the user has something to explore
//! immediately. All lyrics/scripture here are public domain (Amazing Grace,
//! Holy Holy Holy, Be Thou My Vision; KJV John 3:16).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::repositories::{ArrangementRepo, DeckRepo, ServiceRepo, SongRepo};
use crate::db::models::SongInput;
use crate::db::{new_id, now_ms};
use crate::error::AppResult;
use crate::services::slide_doc::SlideDoc;
use sqlx::SqlitePool;

/// A UI language the app offers (Phase 13.1 i18n).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/LocaleInfo.ts")]
pub struct LocaleInfo {
    pub code: String,
    pub name: String,
}

/// Match SundayRec's language set.
pub fn supported_locales() -> Vec<LocaleInfo> {
    [
        ("no", "Norsk"),
        ("en", "English"),
        ("sv", "Svenska"),
        ("da", "Dansk"),
        ("de", "Deutsch"),
        ("fr", "Français"),
        ("pl", "Polski"),
    ]
    .into_iter()
    .map(|(code, name)| LocaleInfo { code: code.into(), name: name.into() })
    .collect()
}

/// What was created, for the onboarding confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/DemoSummary.ts")]
pub struct DemoSummary {
    pub songs: u32,
    pub service_id: String,
    pub deck_id: String,
}

struct DemoSong {
    title: &'static str,
    key: &'static str,
    sections: &'static [(&'static str, &'static str)],
}

const DEMO_SONGS: &[DemoSong] = &[
    DemoSong {
        title: "Amazing Grace",
        key: "G",
        sections: &[
            ("verse_1", "Amazing grace how sweet the sound\nThat saved a wretch like me\nI once was lost but now am found\nWas blind but now I see"),
            ("verse_2", "'Twas grace that taught my heart to fear\nAnd grace my fears relieved\nHow precious did that grace appear\nThe hour I first believed"),
        ],
    },
    DemoSong {
        title: "Holy, Holy, Holy",
        key: "D",
        sections: &[
            ("verse_1", "Holy, holy, holy Lord God Almighty\nEarly in the morning our song shall rise to Thee"),
            ("verse_2", "Holy, holy, holy all the saints adore Thee\nCasting down their golden crowns around the glassy sea"),
        ],
    },
    DemoSong {
        title: "Be Thou My Vision",
        key: "E",
        sections: &[
            ("verse_1", "Be Thou my vision O Lord of my heart\nNaught be all else to me save that Thou art"),
            ("verse_2", "Be Thou my wisdom and Thou my true word\nI ever with Thee and Thou with me Lord"),
        ],
    },
];

/// Prefill `library_id` with the demo content. Returns a summary.
pub async fn seed_demo_content(pool: &SqlitePool, library_id: &str) -> AppResult<DemoSummary> {
    let song_repo = SongRepo::new(pool);
    let arr_repo = ArrangementRepo::new(pool);
    let deck_repo = DeckRepo::new(pool);
    let svc_repo = ServiceRepo::new(pool);

    // Songs + a default arrangement playing their sections in order.
    let mut song_ids = Vec::new();
    let mut default_arrangements = Vec::new();
    for ds in DEMO_SONGS {
        let song = song_repo
            .create(SongInput {
                library_id: library_id.to_string(),
                title: ds.title.to_string(),
                language: Some("en".into()),
                default_key: Some(ds.key.to_string()),
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: Some("Public Domain".into()),
            })
            .await?;
        let mut section_ids = Vec::new();
        for (label, lyrics) in ds.sections {
            let s = song_repo.add_section(&song.id, label, lyrics).await?;
            section_ids.push(s.id);
        }
        let arr = arr_repo.create(&song.id, "Standard").await?;
        arr_repo.set_items(&arr.id, &section_ids).await?;
        song_ids.push(song.id);
        default_arrangements.push(arr.id);
    }

    // A cached scripture (KJV John 3:16 — public domain).
    let bible_id = new_id();
    let now = now_ms();
    sqlx::query(
        r#"
        INSERT INTO bible_reference (id, book, chapter, verse_start, verse_end, translation, text, created_at)
        VALUES (?1, 'John', 3, 16, NULL, 'KJV', ?2, ?3)
        "#,
    )
    .bind(&bible_id)
    .bind("For God so loved the world, that he gave his only begotten Son,\nthat whosoever believeth in him should not perish, but have everlasting life.")
    .bind(now)
    .execute(pool)
    .await?;

    // A welcome deck with a single notes slide.
    let deck = deck_repo.create_deck(library_id, "Velkommen").await?;
    deck_repo
        .create_slide(&deck.id, &SlideDoc::with_lines("welcome", &["Velkommen til gudstjenesten".to_string()]))
        .await?;

    // The Welcome Service tying it together.
    let service = svc_repo.create(library_id, "Velkomstgudstjeneste", now).await?;
    let mut pos = 0i64;
    svc_repo.add_item(&service.id, pos, "announcement", None, None, None, None, Some("Velkommen!")).await?;
    pos += 1;
    for (song_id, arr_id) in song_ids.iter().zip(default_arrangements.iter()) {
        svc_repo
            .add_item(&service.id, pos, "song", Some(song_id), Some(arr_id), None, None, None)
            .await?;
        pos += 1;
    }
    svc_repo.add_item(&service.id, pos, "scripture", None, None, None, Some(&bible_id), None).await?;

    Ok(DemoSummary {
        songs: song_ids.len() as u32,
        service_id: service.id,
        deck_id: deck.id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::LibraryInput;
    use crate::db::repositories::LibraryRepo;
    use crate::db::Database;
    use crate::services::cue_list::CueCompiler;

    #[test]
    fn seven_supported_locales_with_norwegian_first() {
        let locales = supported_locales();
        assert_eq!(locales.len(), 7);
        assert_eq!(locales[0].code, "no");
        assert!(locales.iter().any(|l| l.code == "en"));
    }

    #[tokio::test]
    async fn seed_creates_playable_welcome_service() {
        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput { name: "Personal".into(), default_locale: None })
            .await
            .unwrap();

        let summary = seed_demo_content(&db.pool, &lib.id).await.unwrap();
        assert_eq!(summary.songs, 3);

        // Library has the three songs.
        let songs = SongRepo::new(&db.pool).list(&lib.id, 50, 0).await.unwrap();
        assert_eq!(songs.len(), 3);

        // The welcome service compiles into a non-empty, playable cue list.
        let cl = CueCompiler::new(&db.pool).compile(&summary.service_id).await.unwrap();
        assert!(!cl.is_empty(), "demo service should produce cues");

        // The welcome deck has a slide.
        let slides = DeckRepo::new(&db.pool).list_slides(&summary.deck_id).await.unwrap();
        assert_eq!(slides.len(), 1);
    }
}
