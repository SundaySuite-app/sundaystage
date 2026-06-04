//! Phase: deep-stage-1 — verse-aware scripture auto-break.
//!
//! The old cue compiler chunked a cached scripture passage by raw line count
//! (`text.lines().chunks(N)`), which happily split a verse across two slides and
//! produced ugly orphans. This module replaces that with a *verse-aware* splitter:
//!
//!   - whole verses are kept together on a slide — never broken mid-verse,
//!   - …unless a single verse alone exceeds the line budget, in which case that
//!     one verse spills across as many slides as it needs (there is no other
//!     option), and the verses around it are unaffected,
//!   - verse order is preserved, including across chapter boundaries
//!     (e.g. a reference that spans Psalm 19:1–14 in one chapter, or a passage
//!     crossing from chapter 3 into chapter 4), and
//!   - the reference label is stamped onto every produced slide.
//!
//! Everything here is PURE and deterministic: it takes already-resolved verse
//! data + a line budget and returns slides. The runtime adapter
//! [`verses_from_reference`] reconstructs the verse list from the per-service
//! cached [`BibleReference`] (which is stored one-verse-per-line by
//! `bible_add_to_service`), so the cue compiler can feed it straight in.

use crate::db::models::BibleReference;

/// One scripture verse, pre-split into the display lines it should keep together.
///
/// `lines` is normally a single entry (one verse = one line of text), but the
/// type allows a verse that already carries hard line breaks (e.g. poetry) to
/// stay grouped. Empty/blank lines are not represented here — the adapter drops
/// them so they never become an empty slide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verse {
    pub chapter: i64,
    pub number: i64,
    pub lines: Vec<String>,
}

impl Verse {
    fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// A single auto-broken scripture slide: the lines to show plus the reference
/// label that rides along on every slide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptureSlide {
    pub reference_label: String,
    pub lines: Vec<String>,
}

/// Break a verse-grouped passage into slides that respect `lines_per_slide`.
///
/// Rules (see module docs): group whole verses up to the budget; a verse that on
/// its own exceeds the budget is split across slides by line; verse order is
/// preserved across chapters; the `reference_label` is stamped on every slide.
///
/// Pure + deterministic: same inputs ⇒ byte-identical output, no clock/DB/RNG.
/// An empty passage yields zero slides (the caller decides whether to emit a
/// placeholder cue).
pub fn break_passage(
    verses: &[Verse],
    reference_label: &str,
    lines_per_slide: usize,
) -> Vec<ScriptureSlide> {
    let budget = lines_per_slide.max(1);
    let mut slides: Vec<ScriptureSlide> = Vec::new();
    // The lines accumulated for the slide currently being built.
    let mut current: Vec<String> = Vec::new();

    let flush = |current: &mut Vec<String>, slides: &mut Vec<ScriptureSlide>| {
        if !current.is_empty() {
            slides.push(ScriptureSlide {
                reference_label: reference_label.to_string(),
                lines: std::mem::take(current),
            });
        }
    };

    for verse in verses {
        if verse.lines.is_empty() {
            continue;
        }

        // A verse that cannot fit on a slide by itself must spill across slides.
        // Close any in-progress slide first so the long verse starts clean, then
        // emit full-budget slides until it's consumed. This is the *only* case
        // where we break mid-verse — there is no alternative.
        if verse.line_count() > budget {
            flush(&mut current, &mut slides);
            for chunk in verse.lines.chunks(budget) {
                slides.push(ScriptureSlide {
                    reference_label: reference_label.to_string(),
                    lines: chunk.to_vec(),
                });
            }
            continue;
        }

        // Otherwise keep the verse whole: if it doesn't fit alongside what we've
        // already gathered, close the current slide and start a fresh one.
        if current.len() + verse.line_count() > budget {
            flush(&mut current, &mut slides);
        }
        current.extend(verse.lines.iter().cloned());
    }

    flush(&mut current, &mut slides);
    slides
}

/// Reconstruct the verse list from a per-service cached [`BibleReference`].
///
/// `bible_add_to_service` caches the chosen passage as one verse per line
/// (`verses.join("\n")`), starting at `verse_start`, for a single `chapter`.
/// We mirror that contract here: each non-blank line becomes one [`Verse`],
/// numbered sequentially from `verse_start` within `reference.chapter`.
///
/// Honesty note: the cache does not persist explicit verse numbers or chapter
/// boundaries, so this adapter can only reconstruct a single-chapter passage.
/// The pure [`break_passage`] fully handles multi-chapter input — once the cache
/// gains per-verse metadata (a later step), only this adapter changes.
pub fn verses_from_reference(reference: &BibleReference) -> Vec<Verse> {
    let start = reference.verse_start.max(1);
    reference
        .text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, line)| Verse {
            chapter: reference.chapter,
            number: start + i as i64,
            lines: vec![line.to_string()],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(chapter: i64, number: i64, text: &str) -> Verse {
        Verse {
            chapter,
            number,
            lines: vec![text.to_string()],
        }
    }

    fn ref_fixture(
        book: &str,
        chapter: i64,
        start: i64,
        end: Option<i64>,
        text: &str,
    ) -> BibleReference {
        BibleReference {
            id: "ref-1".into(),
            book: book.into(),
            chapter,
            verse_start: start,
            verse_end: end,
            translation: "KJV".into(),
            text: text.into(),
            created_at: 0,
        }
    }

    #[test]
    fn exact_breakdown_for_known_multi_verse_passage() {
        // Six one-line verses at a budget of 4 → [v1..v4], [v5,v6].
        let verses: Vec<Verse> = (1..=6).map(|n| v(23, n, &format!("line {n}"))).collect();
        let slides = break_passage(&verses, "Psalms 23:1-6", 4);
        assert_eq!(slides.len(), 2);
        assert_eq!(
            slides[0].lines,
            vec!["line 1", "line 2", "line 3", "line 4"]
        );
        assert_eq!(slides[1].lines, vec!["line 5", "line 6"]);
    }

    #[test]
    fn never_breaks_mid_verse_when_verse_fits() {
        // Two 2-line verses at budget 3: the second verse can't share the first
        // slide (2 + 2 > 3), so it moves whole to slide 2 rather than splitting.
        let verses = vec![
            Verse {
                chapter: 1,
                number: 1,
                lines: vec!["1a".into(), "1b".into()],
            },
            Verse {
                chapter: 1,
                number: 2,
                lines: vec!["2a".into(), "2b".into()],
            },
        ];
        let slides = break_passage(&verses, "Book 1:1-2", 3);
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].lines, vec!["1a", "1b"]);
        assert_eq!(slides[1].lines, vec!["2a", "2b"]);
    }

    #[test]
    fn single_overlong_verse_gets_its_own_slides() {
        // A 5-line verse at budget 2, sandwiched between normal verses.
        let verses = vec![
            v(1, 1, "before"),
            Verse {
                chapter: 1,
                number: 2,
                lines: (1..=5).map(|n| format!("big {n}")).collect(),
            },
            v(1, 3, "after"),
        ];
        let slides = break_passage(&verses, "Book 1:1-3", 2);
        // v1 alone (flushed when the overlong verse starts) → 1 slide.
        // overlong v2 (5 lines / 2) → 3 slides ([1,2],[3,4],[5]).
        // v3 alone → 1 slide. Total 5.
        assert_eq!(slides.len(), 5);
        assert_eq!(slides[0].lines, vec!["before"]);
        assert_eq!(slides[1].lines, vec!["big 1", "big 2"]);
        assert_eq!(slides[2].lines, vec!["big 3", "big 4"]);
        assert_eq!(slides[3].lines, vec!["big 5"]);
        assert_eq!(slides[4].lines, vec!["after"]);
    }

    #[test]
    fn multi_chapter_preserves_order() {
        // A passage crossing a chapter boundary: 3:38, 3:39, 4:1, 4:2.
        let verses = vec![
            v(3, 38, "c3 v38"),
            v(3, 39, "c3 v39"),
            v(4, 1, "c4 v1"),
            v(4, 2, "c4 v2"),
        ];
        let slides = break_passage(&verses, "Luke 3:38-4:2", 2);
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].lines, vec!["c3 v38", "c3 v39"]);
        assert_eq!(slides[1].lines, vec!["c4 v1", "c4 v2"]);
    }

    #[test]
    fn reference_label_present_on_every_slide() {
        let verses: Vec<Verse> = (1..=14).map(|n| v(19, n, &format!("v{n}"))).collect();
        let slides = break_passage(&verses, "Psalm 19:1-14", 4);
        assert!(slides.len() >= 4);
        assert!(slides.iter().all(|s| s.reference_label == "Psalm 19:1-14"));
    }

    #[test]
    fn deterministic_same_input_same_output() {
        let verses: Vec<Verse> = (1..=9).map(|n| v(1, n, &format!("line {n}"))).collect();
        let a = break_passage(&verses, "Book 1:1-9", 4);
        let b = break_passage(&verses, "Book 1:1-9", 4);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_passage_yields_no_slides() {
        let slides = break_passage(&[], "Nowhere 0:0", 4);
        assert!(slides.is_empty());
    }

    #[test]
    fn one_verse_yields_one_slide() {
        let slides = break_passage(&[v(3, 16, "For God so loved the world")], "John 3:16", 4);
        assert_eq!(slides.len(), 1);
        assert_eq!(slides[0].lines, vec!["For God so loved the world"]);
        assert_eq!(slides[0].reference_label, "John 3:16");
    }

    #[test]
    fn budget_zero_is_clamped_to_one() {
        let verses = vec![v(1, 1, "a"), v(1, 2, "b")];
        let slides = break_passage(&verses, "Book 1:1-2", 0);
        // Clamp to 1 line per slide → one slide per verse.
        assert_eq!(slides.len(), 2);
    }

    // ── adapter ──────────────────────────────────────────────────────────────

    #[test]
    fn adapter_numbers_verses_from_verse_start() {
        let r = ref_fixture(
            "John",
            3,
            16,
            Some(17),
            "For God so loved the world\nthat he gave his one and only Son",
        );
        let verses = verses_from_reference(&r);
        assert_eq!(verses.len(), 2);
        assert_eq!(verses[0], v(3, 16, "For God so loved the world"));
        assert_eq!(verses[1], v(3, 17, "that he gave his one and only Son"));
    }

    #[test]
    fn adapter_skips_blank_lines() {
        let r = ref_fixture("Psalms", 23, 1, Some(2), "first\n\n  \nsecond\n");
        let verses = verses_from_reference(&r);
        assert_eq!(verses.len(), 2);
        assert_eq!(verses[0].number, 1);
        assert_eq!(verses[1].number, 2);
        assert_eq!(verses[1].lines, vec!["second"]);
    }

    #[test]
    fn adapter_clamps_verse_start_below_one() {
        let r = ref_fixture("Genesis", 1, 0, None, "In the beginning");
        let verses = verses_from_reference(&r);
        assert_eq!(verses[0].number, 1);
    }

    #[test]
    fn adapter_then_break_round_trip() {
        let r = ref_fixture("Psalms", 19, 1, Some(4), "v one\nv two\nv three\nv four");
        let verses = verses_from_reference(&r);
        let slides = break_passage(&verses, "Psalms 19:1-4", 3);
        // 4 single-line verses at budget 3 → [v1,v2,v3], [v4].
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].lines, vec!["v one", "v two", "v three"]);
        assert_eq!(slides[1].lines, vec!["v four"]);
    }
}
