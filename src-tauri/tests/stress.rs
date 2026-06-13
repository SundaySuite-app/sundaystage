//! Phase 6.2 — stress / performance harness.
//!
//! The full plan calls for a Playwright + custom harness driving the real GUI
//! with a CI report and p99 regression gates. That needs a windowing session
//! and a browser, which aren't available headless — so this harness covers the
//! parts that *are* verifiable without a display: the data layer and the live
//! runtime under load. Each test prints its timing (visible with
//! `cargo test --test stress -- --nocapture`) and asserts generous bounds so it
//! is informative without being flaky.
//!
//! Run: `cargo test --test stress`

use std::time::Instant;

use sundaystage_lib::db::models::{LibraryInput, SongInput};
use sundaystage_lib::db::repositories::{ArrangementRepo, LibraryRepo, SongRepo};
use sundaystage_lib::db::Database;
use sundaystage_lib::services::cue_list::{Cue, CueList, CueSource, SlideContent};
use sundaystage_lib::services::live_session::{LiveAction, LiveSession};

fn song_input(library_id: &str, title: &str) -> SongInput {
    SongInput {
        library_id: library_id.to_string(),
        title: title.to_string(),
        language: None,
        default_key: None,
        tempo_bpm: None,
        ccli_song_id: None,
        tono_work_id: None,
        copyright_notice: None,
    }
}

fn slide_cue(i: usize) -> Cue {
    Cue::ShowSlide {
        cue_id: format!("c{i}"),
        slide_content: Box::new(SlideContent {
            section_label: Some(format!("Verse {i}")),
            text_lines: vec![format!("Line {i} a"), format!("Line {i} b")],
            translation_lines: None,
            reference: None,
            sensitive_slide: false,
            appearance: None,
        }),
        theme_id: None,
        template_id: None,
        source: CueSource {
            service_item_id: "item".into(),
            item_cue_index: i as u32,
            display_label: format!("Cue {i}"),
        },
    }
}

/// Scenario 1: full-text search stays fast over a large library.
#[tokio::test]
async fn search_scales_to_thousands_of_songs() {
    let db = Database::open_in_memory().await.unwrap();
    let lib = LibraryRepo::new(&db.pool)
        .create(LibraryInput {
            name: "Stress".into(),
            default_locale: None,
        })
        .await
        .unwrap();
    let repo = SongRepo::new(&db.pool);

    let n = 3000;
    let seed = Instant::now();
    for i in 0..n {
        let song = repo
            .create(song_input(&lib.id, &format!("Song {i}")))
            .await
            .unwrap();
        // Every 7th song mentions "grace" so the search has real hits to rank.
        let lyric = if i % 7 == 0 {
            format!("Amazing grace how sweet line {i}")
        } else {
            format!("Ordinary worship lyric number {i}")
        };
        repo.add_section(&song.id, "verse_1", &lyric).await.unwrap();
    }
    println!("seeded {n} songs in {:?}", seed.elapsed());

    let t = Instant::now();
    let results = repo.search(&lib.id, "grace", 50).await.unwrap();
    let elapsed = t.elapsed();
    println!(
        "FTS search over {n} songs: {} hits in {:?}",
        results.len(),
        elapsed
    );

    assert!(!results.is_empty(), "search should find the 'grace' songs");
    assert!(results.len() <= 50, "limit respected");
    // Generous bound (target is < 100ms; we allow slack to avoid CI flakiness).
    assert!(elapsed.as_millis() < 1000, "search too slow: {elapsed:?}");

    // List pagination over the large set is also quick.
    let t = Instant::now();
    let page = repo.list(&lib.id, 200, 0).await.unwrap();
    println!("list(200) over {n} songs in {:?}", t.elapsed());
    assert_eq!(page.len(), 200);
}

/// Scenario 4: rapid cue advance — the state transition must be effectively
/// instant (the < 50ms keypress→output budget is dominated by render, not this).
#[test]
fn rapid_cue_advance_is_instant() {
    let cues: Vec<Cue> = (0..800).map(slide_cue).collect();
    let cl = CueList {
        service_id: "svc".into(),
        compiled_at: 0,
        cues,
    };
    let mut session = LiveSession::new("svc", cl, 0);

    let t = Instant::now();
    for _ in 0..50 {
        session.dispatch(LiveAction::Next, 0);
    }
    let elapsed = t.elapsed();
    println!("50 cue advances over an 800-cue list in {:?}", elapsed);

    assert_eq!(session.index, 50);
    assert_eq!(session.log.len(), 50);
    assert!(
        elapsed.as_millis() < 50,
        "cue advance too slow: {elapsed:?}"
    );

    // Jump to the end + derive the frame — also instant.
    let t = Instant::now();
    session.dispatch(LiveAction::GoTo { index: 799 }, 0);
    let _frame = session.current_frame();
    println!("goto-end + frame in {:?}", t.elapsed());
    assert_eq!(session.index, 799);
}

/// Scenario 2/3 (data side): a long, repeat-heavy arrangement resolves quickly.
#[tokio::test]
async fn large_arrangement_resolves_quickly() {
    let db = Database::open_in_memory().await.unwrap();
    let lib = LibraryRepo::new(&db.pool)
        .create(LibraryInput {
            name: "Stress".into(),
            default_locale: None,
        })
        .await
        .unwrap();
    let songs = SongRepo::new(&db.pool);
    let song = songs
        .create(song_input(&lib.id, "Long arrangement"))
        .await
        .unwrap();

    // 12 sections, referenced 20× each → a 240-slot arrangement.
    let mut section_ids = Vec::new();
    for i in 0..12 {
        let s = songs
            .add_section(&song.id, &format!("verse_{i}"), &format!("lyric {i}"))
            .await
            .unwrap();
        section_ids.push(s.id);
    }
    let arr_repo = ArrangementRepo::new(&db.pool);
    let arr = arr_repo.create(&song.id, "Full").await.unwrap();
    let sequence: Vec<String> = (0..240).map(|i| section_ids[i % 12].clone()).collect();
    arr_repo.set_items(&arr.id, &sequence).await.unwrap();

    let t = Instant::now();
    let resolved = arr_repo.resolved_sections(&arr.id).await.unwrap();
    println!(
        "resolved a {}-slot arrangement in {:?}",
        resolved.len(),
        t.elapsed()
    );
    assert_eq!(resolved.len(), 240);
}
