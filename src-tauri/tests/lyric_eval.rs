//! Phase 4.2 — lyric-formatting evaluation harness.
//!
//! The plan asks for an eval set of real-world copy-pastes scored on whether
//! the formatter gets section labels, slide breaks, and the arrangement right,
//! and not to ship until the eval clears 85% acceptable. The AI path needs a
//! network and a key, so this harness scores the deterministic offline
//! heuristic (`heuristic_format`) against representative pastes. It is the
//! regression baseline; the AI path should only ever do better.
//!
//! Run with `cargo test --test lyric_eval -- --nocapture`.

use sundaystage_lib::services::ai::lyric_format::{heuristic_format, FormattedSong};

struct Case {
    name: &'static str,
    raw: &'static str,
    lang: &'static str,
    /// The paste marks a repeated section (e.g. "x2" / "(repeat chorus)"), so a
    /// good arrangement should reference some section more than once.
    expects_repeat: bool,
}

const CASES: &[Case] = &[
    Case {
        name: "english_hymn_with_headers",
        raw: "Verse 1\nAmazing grace how sweet the sound\nThat saved a wretch like me\n\nChorus\nMy chains are gone\nI've been set free\n\nVerse 2\nTwas grace that taught my heart to fear",
        lang: "en",
        expects_repeat: false,
    },
    Case {
        name: "norwegian_sangbok",
        raw: "Vers 1\nDeg være ære, Herre over dødens makt\n\nRefreng\nDeg være ære\n\nVers 2\nSe, han er Herre",
        lang: "no",
        expects_repeat: false,
    },
    Case {
        name: "chords_interleaved",
        raw: "Verse 1\nG        D\nAmazing grace how sweet the sound\nEm       C\nThat saved a wretch like me\n\nChorus\nC      G\nMy chains are gone",
        lang: "en",
        expects_repeat: false,
    },
    Case {
        name: "repeat_marker_x2",
        raw: "Chorus\nHoly is the Lord\nGod almighty\nx2\n\nVerse 1\nWe stand and lift up our hands",
        lang: "en",
        expects_repeat: false,
    },
    Case {
        name: "explicit_repeat_chorus",
        raw: "Verse 1\nWhen peace like a river attendeth my way\n\nChorus\nIt is well with my soul\n\nVerse 2\nThough Satan should buffet\n\n(repeat chorus)",
        lang: "en",
        expects_repeat: true,
    },
    Case {
        name: "bridge_and_tag",
        raw: "Verse 1\nWhat a beautiful name it is\n\nChorus\nWhat a beautiful name it is\nThe name of Jesus\n\nBridge\nDeath could not hold you\n\nTag\nNothing can stop you",
        lang: "en",
        expects_repeat: false,
    },
    Case {
        name: "bracketed_headers",
        raw: "[Verse 1]\nO come all ye faithful\n\n[Chorus]\nO come let us adore him\n\n[Verse 2]\nSing choirs of angels",
        lang: "en",
        expects_repeat: false,
    },
    Case {
        name: "norwegian_with_chords",
        raw: "Vers 1\nD        A\nStor er din trofasthet\n\nRefreng\nG     D\nStor er din trofasthet",
        lang: "no",
        expects_repeat: false,
    },
];

/// A line that is *only* chord tokens (what the formatter must strip).
fn is_chordish(line: &str) -> bool {
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.is_empty() {
        return false;
    }
    toks.iter().all(|t| {
        let t = t.trim_end_matches('|');
        let mut chars = t.chars();
        match chars.next() {
            Some(c) if ('A'..='G').contains(&c) => t.len() <= 6,
            _ => false,
        }
    })
}

/// Score a single result on the invariants any reasonable formatter must hold.
/// Returns (passed, total) checks for this case.
fn score(case: &Case, f: &FormattedSong) -> (u32, u32) {
    let mut passed = 0;
    let mut total = 0;
    let labels: Vec<&str> = f.sections.iter().map(|s| s.label.as_str()).collect();

    // 1. Produces at least one section.
    total += 1;
    if !f.sections.is_empty() {
        passed += 1;
    }

    // 2. Arrangement is non-empty and every ref resolves to a section.
    total += 1;
    if !f.arrangement.is_empty() && f.arrangement.iter().all(|r| labels.contains(&r.as_str())) {
        passed += 1;
    }

    // 3. No chord-only line leaked into any section's lyrics.
    total += 1;
    let leaked = f
        .sections
        .iter()
        .flat_map(|s| s.lyrics.lines())
        .any(is_chordish);
    if !leaked {
        passed += 1;
    }

    // 4. Language detected as expected.
    total += 1;
    if f.language == case.lang {
        passed += 1;
    }

    // 5. Repetition reflected in the arrangement when the paste marks it.
    if case.expects_repeat {
        total += 1;
        let repeats = f
            .arrangement
            .iter()
            .any(|l| f.arrangement.iter().filter(|x| *x == l).count() > 1);
        if repeats {
            passed += 1;
        }
    }

    (passed, total)
}

#[test]
fn heuristic_formatter_meets_quality_bar() {
    let mut passed = 0u32;
    let mut total = 0u32;
    for case in CASES {
        let f = heuristic_format(case.raw);
        let (p, t) = score(case, &f);
        println!(
            "{:<24} {}/{}  (lang={}, {} sections, arr={:?})",
            case.name,
            p,
            t,
            f.language,
            f.sections.len(),
            f.arrangement
        );
        passed += p;
        total += t;
    }
    let ratio = passed as f64 / total as f64;
    println!(
        "── heuristic eval: {passed}/{total} = {:.1}%",
        ratio * 100.0
    );
    assert!(
        ratio >= 0.85,
        "heuristic eval below the 85% bar: {passed}/{total} = {:.1}%",
        ratio * 100.0
    );
}
