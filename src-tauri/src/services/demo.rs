//! Phase 13.1 — first-run demo content + supported locales.
//!
//! On first launch the onboarding flow can prefill a library with a small,
//! ready-to-play "Velkomstgudstjeneste" so the user has something to explore
//! immediately. All lyrics/scripture here are public domain (Amazing Grace,
//! Holy Holy Holy, Be Thou My Vision; KJV John 3:16).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models::SongInput;
use crate::db::repositories::{ArrangementRepo, DeckRepo, ServiceRepo, SongRepo};
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
    .map(|(code, name)| LocaleInfo {
        code: code.into(),
        name: name.into(),
    })
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
    lang: &'static str,
    key: &'static str,
    sections: &'static [(&'static str, &'static str)],
}

/// A public-domain starter library. All texts are pre-1925 hymns whose words
/// are in the public domain; users are still prompted to confirm their own
/// licensing. The first three double as the demo Welcome Service.
const STARTER_SONGS: &[DemoSong] = &[
    DemoSong {
        title: "Amazing Grace",
        lang: "en",
        key: "G",
        sections: &[
            ("verse_1", "Amazing grace how sweet the sound\nThat saved a wretch like me\nI once was lost but now am found\nWas blind but now I see"),
            ("verse_2", "'Twas grace that taught my heart to fear\nAnd grace my fears relieved\nHow precious did that grace appear\nThe hour I first believed"),
        ],
    },
    DemoSong {
        title: "Holy, Holy, Holy",
        lang: "en",
        key: "D",
        sections: &[
            ("verse_1", "Holy, holy, holy Lord God Almighty\nEarly in the morning our song shall rise to Thee"),
            ("verse_2", "Holy, holy, holy all the saints adore Thee\nCasting down their golden crowns around the glassy sea"),
        ],
    },
    DemoSong {
        title: "Be Thou My Vision",
        lang: "en",
        key: "E",
        sections: &[
            ("verse_1", "Be Thou my vision O Lord of my heart\nNaught be all else to me save that Thou art"),
            ("verse_2", "Be Thou my wisdom and Thou my true word\nI ever with Thee and Thou with me Lord"),
        ],
    },
    DemoSong {
        title: "Come Thou Fount of Every Blessing",
        lang: "en",
        key: "D",
        sections: &[
            ("verse_1", "Come Thou Fount of every blessing\nTune my heart to sing Thy grace\nStreams of mercy never ceasing\nCall for songs of loudest praise"),
        ],
    },
    DemoSong {
        title: "Crown Him with Many Crowns",
        lang: "en",
        key: "D",
        sections: &[
            ("verse_1", "Crown Him with many crowns\nThe Lamb upon His throne\nHark how the heavenly anthem drowns\nAll music but its own"),
        ],
    },
    DemoSong {
        title: "O for a Thousand Tongues to Sing",
        lang: "en",
        key: "G",
        sections: &[
            ("verse_1", "O for a thousand tongues to sing\nMy great Redeemer's praise\nThe glories of my God and King\nThe triumphs of His grace"),
        ],
    },
    DemoSong {
        title: "Rock of Ages",
        lang: "en",
        key: "A",
        sections: &[
            ("verse_1", "Rock of Ages, cleft for me\nLet me hide myself in Thee\nLet the water and the blood\nFrom Thy wounded side which flowed\nBe of sin the double cure\nSave from wrath and make me pure"),
        ],
    },
    DemoSong {
        title: "It Is Well with My Soul",
        lang: "en",
        key: "C",
        sections: &[
            ("verse_1", "When peace like a river attendeth my way\nWhen sorrows like sea billows roll\nWhatever my lot, Thou hast taught me to say\nIt is well, it is well with my soul"),
            ("chorus", "It is well with my soul\nIt is well, it is well with my soul"),
        ],
    },
    DemoSong {
        title: "What a Friend We Have in Jesus",
        lang: "en",
        key: "F",
        sections: &[
            ("verse_1", "What a friend we have in Jesus\nAll our sins and griefs to bear\nWhat a privilege to carry\nEverything to God in prayer"),
        ],
    },
    DemoSong {
        title: "Blessed Assurance",
        lang: "en",
        key: "D",
        sections: &[
            ("verse_1", "Blessed assurance, Jesus is mine\nO what a foretaste of glory divine\nHeir of salvation, purchase of God\nBorn of His Spirit, washed in His blood"),
            ("chorus", "This is my story, this is my song\nPraising my Savior all the day long"),
        ],
    },
    DemoSong {
        title: "To God Be the Glory",
        lang: "en",
        key: "A",
        sections: &[
            ("verse_1", "To God be the glory, great things He hath done\nSo loved He the world that He gave us His Son"),
            ("chorus", "Praise the Lord, praise the Lord, let the earth hear His voice\nPraise the Lord, praise the Lord, let the people rejoice"),
        ],
    },
    DemoSong {
        title: "Praise to the Lord, the Almighty",
        lang: "en",
        key: "G",
        sections: &[
            ("verse_1", "Praise to the Lord, the Almighty, the King of creation\nO my soul, praise Him, for He is thy health and salvation"),
        ],
    },
    DemoSong {
        title: "All Hail the Power of Jesus' Name",
        lang: "en",
        key: "E",
        sections: &[
            ("verse_1", "All hail the power of Jesus' name\nLet angels prostrate fall\nBring forth the royal diadem\nAnd crown Him Lord of all"),
        ],
    },
    DemoSong {
        title: "Immortal, Invisible, God Only Wise",
        lang: "en",
        key: "F",
        sections: &[
            ("verse_1", "Immortal, invisible, God only wise\nIn light inaccessible hid from our eyes\nMost blessed, most glorious, the Ancient of Days\nAlmighty, victorious, Thy great name we praise"),
        ],
    },
    DemoSong {
        title: "Deilig er jorden",
        lang: "no",
        key: "C",
        sections: &[
            ("verse_1", "Deilig er jorden, prektig er Guds himmel\nskjønn er sjelenes pilegrimsgang\nGjennom de fagre riker på jorden\ngår vi til paradis med sang"),
        ],
    },
    DemoSong {
        title: "Deilig er den himmel blå",
        lang: "no",
        key: "D",
        sections: &[
            ("verse_1", "Deilig er den himmel blå\nlyst det er å se derpå\nhvor de gylne stjerner blinker\nhvor de smiler, hvor de vinker\noss fra jorden opp til seg"),
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
    for ds in STARTER_SONGS {
        let song = song_repo
            .create(SongInput {
                library_id: library_id.to_string(),
                title: ds.title.to_string(),
                language: Some(ds.lang.into()),
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
        .create_slide(
            &deck.id,
            &SlideDoc::with_lines("welcome", &["Velkommen til gudstjenesten".to_string()]),
        )
        .await?;

    // The Welcome Service tying it together.
    let service = svc_repo
        .create(library_id, "Velkomstgudstjeneste", now)
        .await?;
    let mut pos = 0i64;
    svc_repo
        .add_item(
            &service.id,
            pos,
            "announcement",
            None,
            None,
            None,
            None,
            Some("Velkommen!"),
        )
        .await?;
    pos += 1;
    // The Welcome Service stays short: the first three starter songs.
    for (song_id, arr_id) in song_ids.iter().zip(default_arrangements.iter()).take(3) {
        svc_repo
            .add_item(
                &service.id,
                pos,
                "song",
                Some(song_id),
                Some(arr_id),
                None,
                None,
                None,
            )
            .await?;
        pos += 1;
    }
    svc_repo
        .add_item(
            &service.id,
            pos,
            "scripture",
            None,
            None,
            None,
            Some(&bible_id),
            None,
        )
        .await?;

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
            .create(LibraryInput {
                name: "Personal".into(),
                default_locale: None,
            })
            .await
            .unwrap();

        let summary = seed_demo_content(&db.pool, &lib.id).await.unwrap();
        assert_eq!(summary.songs, STARTER_SONGS.len() as u32);

        // The full starter library is installed.
        let songs = SongRepo::new(&db.pool).list(&lib.id, 50, 0).await.unwrap();
        assert_eq!(songs.len(), STARTER_SONGS.len());

        // The welcome service compiles into a non-empty, playable cue list.
        let cl = CueCompiler::new(&db.pool)
            .compile(&summary.service_id)
            .await
            .unwrap();
        assert!(!cl.is_empty(), "demo service should produce cues");

        // The welcome deck has a slide.
        let slides = DeckRepo::new(&db.pool)
            .list_slides(&summary.deck_id)
            .await
            .unwrap();
        assert_eq!(slides.len(), 1);
    }
}
