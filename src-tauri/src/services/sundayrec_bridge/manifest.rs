//! Phase 10.3 — Stage → SundayRec service-manifest export.
//!
//! [`chapter_markers`] and [`session_to_srt`](super::export) already turn a live
//! session into a recording timeline, but they carry no *identity*: a song's
//! CCLI/TONO ids — the licensing moat — live in the planning layer, not in the
//! compiled cue. This module joins the session's display timeline back to the
//! service plan (kind + song ids, looked up by `service_item_id`) and emits the
//! `service-manifest.json` shape SundayRec's `stage_import_manifest` parses. The
//! recording then gets chapters *and* a setlist with reportable ids — from a
//! session the operator already ran, no extra work.
//!
//! The join logic is pure: [`build_manifest`] takes a `service_item_id →`
//! [`ItemMeta`] map (the command layer resolves it from the DB) so it stays
//! testable without a database.
//!
//! WIRE CONTRACT: the manifest types are now the CANONICAL ones from the
//! `sunday-contracts` crate (`StageManifest`/`StageManifestItem`/
//! `StageManifestSong`, sunday-platform v0.4.1, `crates/sunday-contracts/src/
//! stage.rs` / `packages/contracts/src/stage.ts`). They were previously a
//! field-identical hand-rolled mirror in this file; that third copy is gone —
//! we re-export the published types (aliased to the old local names so callers
//! are unchanged) and drive the JSON wire shape directly from them. camelCase
//! keys, no `schema_version` envelope, absent options omitted (never `null`).
//! Stage always stamps `source = Some("stage")` (the canonical field is
//! `Option<String>` for the consumer side). Do not add or rename fields without
//! changing the canonical contract first.

use std::collections::HashMap;

// Canonical wire types from the shared contracts crate. We alias them to the
// names this module historically exported so `commands::live` and the tests
// keep compiling unchanged.
pub use sunday_contracts::{
    StageManifest, StageManifestItem as ManifestItem, StageManifestSong as ManifestSong,
    STAGE_MANIFEST_SOURCE,
};

use crate::services::cue_list::Cue;
use crate::services::live_session::{LiveSession, OutputState};

/// Planning-time metadata for one service item, resolved from the DB by the
/// command layer. Passed to [`build_manifest`] as a `service_item_id → ItemMeta`
/// map so the pure builder never touches sqlx. App-local (not a wire type): it
/// carries the canonical [`ManifestSong`] but is never serialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMeta {
    /// The schema kind (`song`, `scripture`, `custom_deck`, …).
    pub kind: String,
    /// The song behind a `song` item, with its licensing ids. `None` for
    /// non-song items (scripture/deck/gap).
    pub song: Option<ManifestSong>,
}

/// One contiguous run of the same service item in the display timeline, with
/// absolute start/end unix ms.
struct ItemSegment {
    service_item_id: String,
    label: String,
    start: i64,
    end: i64,
}

/// Walk the session timeline and produce one segment per contiguous run of the
/// same service item (song/scripture/…), mirroring [`chapter_markers`]'s
/// grouping: a blackout does **not** split a run (the song continues behind the
/// black), while logo / pause / a different item act only as boundaries and
/// never produce a segment of their own (they're operator output, not plan
/// items). End times are the start of the next boundary, or `ended_at` for the
/// last run.
///
/// [`chapter_markers`]: super::export::chapter_markers
fn item_segments(session: &LiveSession, ended_at: i64) -> Vec<ItemSegment> {
    // The display timeline: the first cue shows from `started_at`, then each log
    // entry records the state at its timestamp (mirrors `export::timeline`).
    let mut points: Vec<(i64, usize, OutputState)> =
        vec![(session.started_at, 0, OutputState::Normal)];
    for e in &session.log {
        points.push((e.at, e.index, e.output));
    }

    let mut segments: Vec<ItemSegment> = Vec::new();
    // The currently-open run: (service_item_id, label, start).
    let mut current: Option<(String, String, i64)> = None;

    for (at, index, output) in points {
        // A blackout never splits the active run — skip it entirely.
        if output == OutputState::Blackout {
            continue;
        }
        // Only a Normal slide cue is a service item; logo/pause/anything else is
        // a boundary that closes (but doesn't open) a run.
        let item = match session.cue_list.get(index) {
            Some(Cue::ShowSlide { source, .. }) if output == OutputState::Normal => {
                Some((source.service_item_id.clone(), source.display_label.clone()))
            }
            _ => None,
        };

        match (&current, item) {
            // Same item still showing — extend the open run.
            (Some((cur_id, _, _)), Some((id, _))) if *cur_id == id => {}
            // A new item — close the open run (if any) and open a fresh one.
            (_, Some((id, label))) => {
                if let Some((cid, clabel, cstart)) = current.take() {
                    segments.push(ItemSegment {
                        service_item_id: cid,
                        label: clabel,
                        start: cstart,
                        end: at,
                    });
                }
                current = Some((id, label, at));
            }
            // A boundary (logo/pause) with a run open — close it here.
            (Some(_), None) => {
                let (cid, clabel, cstart) = current.take().expect("run is open");
                segments.push(ItemSegment {
                    service_item_id: cid,
                    label: clabel,
                    start: cstart,
                    end: at,
                });
            }
            // Boundary with nothing open — nothing to do.
            (None, None) => {}
        }
    }

    if let Some((cid, clabel, cstart)) = current.take() {
        segments.push(ItemSegment {
            service_item_id: cid,
            label: clabel,
            start: cstart,
            end: ended_at,
        });
    }

    segments
}

/// Build the [`StageManifest`] for a finished/running session, enriching each
/// service-item run with its planning-time `kind` + song ids from `meta`.
/// `ended_at` closes the final item; `church_id` is threaded through when known.
pub fn build_manifest(
    session: &LiveSession,
    ended_at: i64,
    meta: &HashMap<String, ItemMeta>,
    church_id: Option<String>,
) -> StageManifest {
    let items = item_segments(session, ended_at)
        .into_iter()
        .map(|seg| {
            let entry = meta.get(&seg.service_item_id);
            // Unknown item (deleted since go-live, say) → a faithful "custom"
            // chapter with the operator's display label, no song.
            let kind = entry
                .map(|m| m.kind.clone())
                .unwrap_or_else(|| "custom".to_string());
            let song = entry.and_then(|m| m.song.clone());
            // A song's clean title beats the slide's display label.
            let label = song
                .as_ref()
                .and_then(|s| s.title.clone())
                .unwrap_or(seg.label);
            ManifestItem {
                at_ms: seg.start,
                end_ms: Some(seg.end.max(seg.start)),
                kind,
                label,
                service_item_id: Some(seg.service_item_id),
                song,
            }
        })
        .collect();

    StageManifest {
        // Stage always stamps the canonical producer tag.
        source: Some(STAGE_MANIFEST_SOURCE.to_string()),
        service_id: Some(session.service_id.clone()),
        church_id,
        started_at_ms: session.started_at,
        ended_at_ms: Some(ended_at),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::{Cue, CueList, CueSource, SlideContent};
    use crate::services::live_session::LiveAction;

    fn slide(item: &str, label: &str, line: &str) -> Cue {
        Cue::ShowSlide {
            cue_id: format!("{item}-{label}"),
            slide_content: Box::new(SlideContent {
                section_label: Some(label.to_string()),
                text_lines: vec![line.to_string()],
                translation_lines: None,
                reference: None,
                sensitive_slide: false,
                appearance: None,
            }),
            theme_id: None,
            template_id: None,
            source: CueSource {
                service_item_id: item.to_string(),
                item_cue_index: 0,
                display_label: format!("{item} — {label}"),
            },
        }
    }

    /// Two songs (item A two slides, item B one slide), started at 1000.
    fn two_song_session() -> LiveSession {
        let cues = vec![
            slide("amazing-grace", "Verse 1", "Amazing grace"),
            slide("amazing-grace", "Chorus", "My chains are gone"),
            slide("how-great", "Verse 1", "O Lord my God"),
        ];
        LiveSession::new(
            "svc-1",
            CueList {
                service_id: "svc-1".into(),
                compiled_at: 0,
                cues,
            },
            1_000,
        )
    }

    fn meta() -> HashMap<String, ItemMeta> {
        let mut m = HashMap::new();
        m.insert(
            "amazing-grace".into(),
            ItemMeta {
                kind: "song".into(),
                song: Some(ManifestSong {
                    title: Some("Amazing Grace".into()),
                    tono_work_id: Some("T-123".into()),
                    ccli_song_id: Some("CCLI-22025".into()),
                    sundaysong_id: None,
                }),
            },
        );
        m.insert(
            "how-great".into(),
            ItemMeta {
                kind: "song".into(),
                song: Some(ManifestSong {
                    title: Some("How Great Thou Art".into()),
                    tono_work_id: None,
                    ccli_song_id: Some("CCLI-14181".into()),
                    sundaysong_id: None,
                }),
            },
        );
        m
    }

    #[test]
    fn one_item_per_song_not_per_slide_with_absolute_times() {
        let mut s = two_song_session();
        s.dispatch(LiveAction::Next, 5_000); // still amazing-grace (chorus)
        s.dispatch(LiveAction::Next, 9_000); // how-great
        let manifest = build_manifest(&s, 12_000, &meta(), None);

        assert_eq!(manifest.source.as_deref(), Some("stage"));
        assert_eq!(manifest.service_id.as_deref(), Some("svc-1"));
        assert_eq!(manifest.started_at_ms, 1_000);
        assert_eq!(manifest.ended_at_ms, Some(12_000));
        assert_eq!(manifest.items.len(), 2, "one per song, not per slide");

        let a = &manifest.items[0];
        assert_eq!(a.at_ms, 1_000); // absolute unix ms, not offset
        assert_eq!(a.end_ms, Some(9_000)); // until how-great starts
        assert_eq!(a.kind, "song");
        assert_eq!(a.label, "Amazing Grace"); // song title beats slide label
        assert_eq!(a.service_item_id.as_deref(), Some("amazing-grace"));
        let song = a.song.as_ref().unwrap();
        assert_eq!(song.tono_work_id.as_deref(), Some("T-123"));
        assert_eq!(song.ccli_song_id.as_deref(), Some("CCLI-22025"));

        let b = &manifest.items[1];
        assert_eq!(b.at_ms, 9_000);
        assert_eq!(b.end_ms, Some(12_000)); // ended_at closes the last item
        assert_eq!(b.label, "How Great Thou Art");
    }

    #[test]
    fn blackout_does_not_split_an_item() {
        let mut s = two_song_session();
        s.dispatch(LiveAction::Blackout, 4_000);
        s.dispatch(LiveAction::Blackout, 6_000); // back to the same slide
        let manifest = build_manifest(&s, 8_000, &meta(), None);
        // Still one item for amazing-grace (how-great never reached).
        assert_eq!(manifest.items.len(), 1);
        assert_eq!(
            manifest.items[0].service_item_id.as_deref(),
            Some("amazing-grace")
        );
        assert_eq!(manifest.items[0].end_ms, Some(8_000));
    }

    #[test]
    fn unknown_item_falls_back_to_custom_chapter() {
        // A session item with no DB metadata still produces a faithful chapter.
        let mut s = two_song_session();
        s.dispatch(LiveAction::Next, 5_000);
        s.dispatch(LiveAction::Next, 9_000);
        let mut m = meta();
        m.remove("how-great"); // pretend it was deleted since go-live
        let manifest = build_manifest(&s, 12_000, &m, None);
        assert_eq!(manifest.items.len(), 2);
        let b = &manifest.items[1];
        assert_eq!(b.kind, "custom");
        assert_eq!(b.label, "how-great — Verse 1"); // operator's display label
        assert!(b.song.is_none());
    }

    /// The emitted JSON must deserialize through SundayRec's parser shim. This
    /// replicates `sundayrec-core::integrations::stage::parse_stage_manifest`'s
    /// camelCase shim verbatim so the wire contract is pinned from this side.
    #[test]
    fn json_round_trips_through_sundayrecs_parser_shim() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Shim {
            source: Option<String>,
            service_id: Option<String>,
            started_at_ms: Option<i64>,
            #[serde(default)]
            items: Option<Vec<ShimItem>>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ShimItem {
            at_ms: i64,
            end_ms: Option<i64>,
            kind: String,
            label: String,
            service_item_id: Option<String>,
            song: Option<ShimSong>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ShimSong {
            title: Option<String>,
            tono_work_id: Option<String>,
            ccli_song_id: Option<String>,
            #[allow(dead_code)]
            sundaysong_id: Option<String>,
        }

        let mut s = two_song_session();
        s.dispatch(LiveAction::Next, 5_000);
        s.dispatch(LiveAction::Next, 9_000);
        let json = serde_json::to_string(&build_manifest(&s, 12_000, &meta(), None)).unwrap();

        // Sanity: the wire keys are camelCase (what SundayRec reads).
        assert!(json.contains("\"startedAtMs\""));
        assert!(json.contains("\"atMs\""));
        assert!(json.contains("\"serviceItemId\""));
        assert!(json.contains("\"tonoWorkId\""));
        assert!(json.contains("\"ccliSongId\""));

        let shim: Shim = serde_json::from_str(&json).expect("parses through Rec's shim");
        assert_eq!(shim.source.as_deref(), Some("stage"));
        assert_eq!(shim.service_id.as_deref(), Some("svc-1"));
        assert_eq!(shim.started_at_ms, Some(1_000));
        let items = shim.items.expect("items present");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].at_ms, 1_000);
        assert_eq!(items[0].end_ms, Some(9_000));
        assert_eq!(items[0].kind, "song");
        assert_eq!(items[0].label, "Amazing Grace");
        assert_eq!(items[0].service_item_id.as_deref(), Some("amazing-grace"));
        let song = items[0].song.as_ref().expect("song present");
        assert_eq!(song.title.as_deref(), Some("Amazing Grace"));
        assert_eq!(song.tono_work_id.as_deref(), Some("T-123"));
        assert_eq!(song.ccli_song_id.as_deref(), Some("CCLI-22025"));
    }

    /// Drift guard: the canonical `StageManifest` round-trips byte-for-byte
    /// through serde from JSON the Rec parser produces, AND every field this
    /// builder writes survives. If the contracts crate ever renames/retypes a
    /// field, this fails to compile or fails the assertion — killing silent
    /// drift even though we now consume the type directly.
    #[test]
    fn canonical_type_round_trips_without_field_drift() {
        let mut s = two_song_session();
        s.dispatch(LiveAction::Next, 5_000);
        s.dispatch(LiveAction::Next, 9_000);
        let original = build_manifest(&s, 12_000, &meta(), None);
        let json = serde_json::to_string(&original).unwrap();
        let parsed: StageManifest =
            serde_json::from_str(&json).expect("canonical type deserializes its own JSON");
        assert_eq!(parsed, original, "round-trip must preserve every field");
    }
}
