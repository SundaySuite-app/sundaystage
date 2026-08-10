//! Song EXPORT writers (Spor B5 — the lock-in fix).
//!
//! The import suite (Spor B) makes it painless to move INTO SundayStage; this
//! module makes it painless to move OUT, so a church is never trapped by its
//! song library. Two open, widely-read formats are written from our own model
//! (a [`Song`] + its [`SongSection`]s + a play order):
//!
//!   * **OpenLyrics 0.9** — the interchange XML OpenLP, ProPresenter and others
//!     read. Section labels become verse `name` attributes and the arrangement
//!     becomes `<verseOrder>`, so the file we write round-trips back through our
//!     own OpenLyrics importer with the same sections and order (proven by a
//!     round-trip test).
//!   * **ChordPro** — the plain-text lead-sheet format, with `{title}`/`{ccli}`
//!     directives and `{start_of_verse}`/`{soc}` environments.
//!
//! No crate exists for writing OpenLyrics, so it is written by hand (the plan's
//! A8 note). The mapping between our canonical labels (`verse_1`, `chorus`,
//! `pre_chorus`, …) and OpenLyrics verse names (`v1`, `c`, `p`, …) is the exact
//! inverse of the importer's `code_to_label`, which is what makes the
//! round-trip hold. Praisenter (BSD-3-Clause) has reference writers for both
//! formats; these were reimplemented from the format specs, not copied from its
//! Java — attribution is in `THIRD-PARTY.md`.

use crate::db::models::{Song, SongSection};

/// Serialise a song to OpenLyrics 0.9 XML.
///
/// `sections` are the unique lyric blocks (each becomes one `<verse>`);
/// `arrangement` is the ordered list of section labels (repeats allowed) that
/// becomes `<verseOrder>`. Sections with empty lyrics are skipped, and any
/// `verseOrder` entry pointing at a skipped/unknown section is dropped so the
/// order never dangles.
pub fn to_openlyrics(song: &Song, sections: &[SongSection], arrangement: &[String]) -> String {
    use std::collections::HashSet;

    // Emit one verse per non-empty section; remember the names we wrote.
    let mut verses: Vec<(String, String)> = Vec::new(); // (name, lyrics)
    let mut names: HashSet<String> = HashSet::new();
    for s in sections {
        if s.lyrics.trim().is_empty() {
            continue;
        }
        let name = label_to_openlyrics_name(&s.label);
        if names.insert(name.clone()) {
            verses.push((name, s.lyrics.clone()));
        }
    }

    // verseOrder references only verses that were actually written.
    let order: Vec<String> = arrangement
        .iter()
        .map(|l| label_to_openlyrics_name(l))
        .filter(|n| names.contains(n))
        .collect();

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(
        "<song xmlns=\"http://openlyrics.info/namespace/2009/song\" version=\"0.9\" \
         createdIn=\"SundayStage\">\n",
    );
    out.push_str("  <properties>\n");
    out.push_str("    <titles>\n");
    out.push_str(&format!(
        "      <title>{}</title>\n",
        xml_escape(&song.title)
    ));
    out.push_str("    </titles>\n");
    if let Some(cr) = non_empty(&song.copyright_notice) {
        out.push_str(&format!("    <copyright>{}</copyright>\n", xml_escape(cr)));
    }
    if let Some(ccli) = non_empty(&song.ccli_song_id) {
        out.push_str(&format!("    <ccliNo>{}</ccliNo>\n", xml_escape(ccli)));
    }
    if !order.is_empty() {
        out.push_str(&format!(
            "    <verseOrder>{}</verseOrder>\n",
            xml_escape(&order.join(" "))
        ));
    }
    out.push_str("  </properties>\n");

    out.push_str("  <lyrics>\n");
    for (name, lyrics) in &verses {
        let body = lyrics
            .split('\n')
            .map(xml_escape)
            .collect::<Vec<_>>()
            .join("<br/>");
        out.push_str(&format!(
            "    <verse name=\"{}\">\n      <lines>{}</lines>\n    </verse>\n",
            xml_escape(name),
            body
        ));
    }
    out.push_str("  </lyrics>\n");
    out.push_str("</song>\n");
    out
}

/// Serialise a song to a ChordPro lead sheet.
///
/// Unique sections are written once, in the order they first appear in
/// `arrangement` (then any section the arrangement never references, so nothing
/// is lost). Our model stores no per-line inline chords, so the body is lyrics
/// only.
pub fn to_chordpro(song: &Song, sections: &[SongSection], arrangement: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{{title: {}}}\n", song.title.trim()));
    if let Some(ccli) = non_empty(&song.ccli_song_id) {
        out.push_str(&format!("{{ccli: {ccli}}}\n"));
    }
    if let Some(cr) = non_empty(&song.copyright_notice) {
        // A copyright notice may span lines (imported author + © block); keep it
        // to one directive line so the file stays valid ChordPro.
        out.push_str(&format!("{{copyright: {}}}\n", cr.replace('\n', " ")));
    }

    for section in ordered_unique_sections(sections, arrangement) {
        if section.lyrics.trim().is_empty() {
            continue;
        }
        let (env, has_label) = chordpro_env(&section.label);
        out.push('\n');
        if has_label {
            out.push_str(&format!(
                "{{start_of_{env}: {}}}\n",
                humanize_label(&section.label)
            ));
        } else {
            out.push_str(&format!("{{start_of_{env}}}\n"));
        }
        for line in section.lyrics.split('\n') {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!("{{end_of_{env}}}\n"));
    }
    out
}

// ── mapping helpers ─────────────────────────────────────────────────────────

/// Map a canonical label (`verse_1`, `chorus`, `pre_chorus`, `bridge_2`, …) to
/// an OpenLyrics verse name (`v1`, `c`, `p`, `b2`, …). The exact inverse of the
/// importer's `code_to_label`, so a written file re-imports to the same labels.
/// Unknown words are kept verbatim (they round-trip through `normalize_label`).
fn label_to_openlyrics_name(label: &str) -> String {
    let (word, num) = match label.rsplit_once('_') {
        Some((w, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => (w, n),
        _ => (label, ""),
    };
    let letter = match word {
        "verse" => "v",
        "chorus" => "c",
        "bridge" => "b",
        "pre_chorus" => "p",
        "intro" => "i",
        "ending" => "e",
        "tag" => "t",
        other => other,
    };
    format!("{letter}{num}")
}

/// The ChordPro environment for a label: `verse`/`chorus`/`bridge` map to the
/// three standard environments; everything else uses the generic `verse`
/// environment but carries its human label so the section kind is not lost. The
/// bool is whether to attach that label argument.
fn chordpro_env(label: &str) -> (&'static str, bool) {
    let word = label.rsplit_once('_').map_or(label, |(w, n)| {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            w
        } else {
            label
        }
    });
    match word {
        "verse" => ("verse", true),
        "chorus" => ("chorus", false),
        "bridge" => ("bridge", false),
        _ => ("verse", true),
    }
}

/// Unique sections in play-first order: those referenced by `arrangement`
/// first (in first-appearance order), then any section the arrangement never
/// mentions, in the given display order.
fn ordered_unique_sections<'a>(
    sections: &'a [SongSection],
    arrangement: &[String],
) -> Vec<&'a SongSection> {
    use std::collections::HashMap;
    let by_label: HashMap<&str, &SongSection> =
        sections.iter().map(|s| (s.label.as_str(), s)).collect();

    let mut out: Vec<&SongSection> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for label in arrangement {
        if let Some(s) = by_label.get(label.as_str()) {
            if seen.insert(s.label.as_str()) {
                out.push(s);
            }
        }
    }
    for s in sections {
        if seen.insert(s.label.as_str()) {
            out.push(s);
        }
    }
    out
}

/// Turn a canonical label into a display title: `verse_1` → `Verse 1`,
/// `pre_chorus` → `Pre Chorus`.
fn humanize_label(label: &str) -> String {
    label
        .split('_')
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(first) => first.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn non_empty(opt: &Option<String>) -> Option<&str> {
    opt.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Escape the five XML metacharacters for text and attribute values.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::Song;
    use crate::services::song_import::{parse_song, ImportFormat};

    fn song(title: &str, ccli: Option<&str>, copyright: Option<&str>) -> Song {
        Song {
            id: "song-1".into(),
            library_id: "lib-1".into(),
            title: title.into(),
            ccli_song_id: ccli.map(str::to_string),
            tono_work_id: None,
            copyright_notice: copyright.map(str::to_string),
            default_key: None,
            tempo_bpm: None,
            language: "en".into(),
            last_used_at: None,
            theme_id: None,
            template_id: None,
            created_at: 0,
            updated_at: 0,
            deleted_at: None,
        }
    }

    fn section(label: &str, lyrics: &str, order: i64) -> SongSection {
        SongSection {
            id: format!("sec-{label}"),
            song_id: "song-1".into(),
            label: label.into(),
            lyrics: lyrics.into(),
            chord_chart: None,
            display_order: order,
            created_at: 0,
            updated_at: 0,
        }
    }

    // ── OpenLyrics ─────────────────────────────────────────────────────────────

    #[test]
    fn openlyrics_is_well_formed_and_carries_metadata() {
        let s = song("Amazing Grace", Some("22025"), Some("Public Domain"));
        let secs = vec![
            section(
                "verse_1",
                "Amazing grace how sweet the sound\nThat saved a wretch like me",
                0,
            ),
            section("chorus", "Praise God", 1),
        ];
        let xml = to_openlyrics(
            &s,
            &secs,
            &["verse_1".into(), "chorus".into(), "verse_1".into()],
        );

        // Well-formed: quick-xml reads it end to end without error.
        let mut reader = quick_xml::Reader::from_str(&xml);
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Eof) => break,
                Ok(_) => {}
                Err(e) => panic!("malformed OpenLyrics XML: {e}"),
            }
        }

        assert!(xml.contains("<title>Amazing Grace</title>"));
        assert!(xml.contains("<ccliNo>22025</ccliNo>"));
        assert!(xml.contains("<copyright>Public Domain</copyright>"));
        assert!(xml.contains("<verseOrder>v1 c v1</verseOrder>"));
        assert!(xml.contains("<verse name=\"v1\">"));
        assert!(xml.contains("Amazing grace how sweet the sound<br/>That saved a wretch like me"));
    }

    #[test]
    fn openlyrics_round_trips_through_the_importer() {
        let s = song("Be Thou My Vision", Some("30639"), None);
        let secs = vec![
            section("verse_1", "Be thou my vision\nO Lord of my heart", 0),
            section("chorus", "Naught be all else to me", 1),
            section("bridge", "High King of heaven", 2),
        ];
        let arrangement = vec![
            "verse_1".to_string(),
            "chorus".to_string(),
            "bridge".to_string(),
            "chorus".to_string(),
        ];
        let xml = to_openlyrics(&s, &secs, &arrangement);

        // Export → import must yield the SAME sections and play order.
        let reparsed = parse_song(&xml, ImportFormat::OpenLyrics);
        assert_eq!(
            reparsed.title_suggestion.as_deref(),
            Some("Be Thou My Vision")
        );
        let got: Vec<(&str, &str)> = reparsed
            .sections
            .iter()
            .map(|x| (x.label.as_str(), x.lyrics.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("verse_1", "Be thou my vision\nO Lord of my heart"),
                ("chorus", "Naught be all else to me"),
                ("bridge", "High King of heaven"),
            ]
        );
        assert_eq!(reparsed.arrangement, arrangement);
    }

    #[test]
    fn openlyrics_round_trips_numbered_and_special_labels() {
        let s = song("Labels", None, None);
        let secs = vec![
            section("verse_1", "one", 0),
            section("verse_2", "two", 1),
            section("pre_chorus", "pre", 2),
            section("chorus", "the chorus", 3),
            section("bridge", "the bridge", 4),
            section("tag", "the tag", 5),
            section("ending", "the end", 6),
            section("instrumental", "solo", 7),
        ];
        let labels: Vec<String> = secs.iter().map(|x| x.label.clone()).collect();
        let xml = to_openlyrics(&s, &secs, &labels);
        let reparsed = parse_song(&xml, ImportFormat::OpenLyrics);
        assert_eq!(
            reparsed
                .sections
                .iter()
                .map(|x| x.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "verse_1",
                "verse_2",
                "pre_chorus",
                "chorus",
                "bridge",
                "tag",
                "ending",
                "instrumental"
            ]
        );
    }

    #[test]
    fn openlyrics_round_trips_a_repeated_chorus_label() {
        // Two distinct chorus sections (`chorus` + `chorus_2`) both map to the
        // `c`/`c2` name space; on import the label system re-disambiguates the
        // second to `chorus_2`, so the pair survives the round trip.
        let s = song("Two Choruses", None, None);
        let secs = vec![
            section("verse_1", "v", 0),
            section("chorus", "first chorus", 1),
            section("chorus_2", "second chorus", 2),
        ];
        let labels: Vec<String> = secs.iter().map(|x| x.label.clone()).collect();
        let xml = to_openlyrics(&s, &secs, &labels);
        let reparsed = parse_song(&xml, ImportFormat::OpenLyrics);
        let got: Vec<(&str, &str)> = reparsed
            .sections
            .iter()
            .map(|x| (x.label.as_str(), x.lyrics.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("verse_1", "v"),
                ("chorus", "first chorus"),
                ("chorus_2", "second chorus"),
            ]
        );
    }

    #[test]
    fn openlyrics_escapes_xml_metacharacters() {
        let s = song("Holy & <Mighty>", None, None);
        let secs = vec![section("verse_1", "Praise \"Him\" & 'sing'", 0)];
        let xml = to_openlyrics(&s, &secs, &[]);
        assert!(xml.contains("<title>Holy &amp; &lt;Mighty&gt;</title>"));
        // And it re-imports with the characters decoded back.
        let reparsed = parse_song(&xml, ImportFormat::OpenLyrics);
        assert_eq!(
            reparsed.title_suggestion.as_deref(),
            Some("Holy & <Mighty>")
        );
        assert_eq!(reparsed.sections[0].lyrics, "Praise \"Him\" & 'sing'");
    }

    #[test]
    fn openlyrics_empty_song_is_well_formed_with_no_verses() {
        let s = song("Empty", None, None);
        let xml = to_openlyrics(&s, &[], &[]);
        assert!(xml.contains("<title>Empty</title>"));
        assert!(!xml.contains("<verse "));
        assert!(!xml.contains("<verseOrder>"));
        // Re-imports to a warned, section-less stub (no panic).
        let reparsed = parse_song(&xml, ImportFormat::OpenLyrics);
        assert!(reparsed.sections.is_empty());
        assert!(!reparsed.warnings.is_empty());
    }

    // ── ChordPro ───────────────────────────────────────────────────────────────

    #[test]
    fn chordpro_has_directives_and_environments() {
        let s = song("Amazing Grace", Some("22025"), Some("Public Domain"));
        let secs = vec![
            section("verse_1", "Amazing grace\nhow sweet the sound", 0),
            section("chorus", "Praise God", 1),
            section("bridge", "My chains are gone", 2),
        ];
        let cho = to_chordpro(&s, &secs, &["verse_1".into(), "chorus".into()]);
        assert!(cho.contains("{title: Amazing Grace}"));
        assert!(cho.contains("{ccli: 22025}"));
        assert!(cho.contains("{copyright: Public Domain}"));
        assert!(cho.contains("{start_of_verse: Verse 1}"));
        assert!(cho.contains("{end_of_verse}"));
        assert!(cho.contains("{start_of_chorus}"));
        assert!(cho.contains("{end_of_chorus}"));
        assert!(cho.contains("{start_of_bridge}"));
        assert!(cho.contains("Amazing grace\nhow sweet the sound"));
    }

    #[test]
    fn chordpro_orders_sections_by_arrangement_then_leftovers() {
        let s = song("Order", None, None);
        let secs = vec![
            section("verse_1", "v1", 0),
            section("chorus", "c", 1),
            section("verse_2", "v2", 2), // never referenced by the arrangement
        ];
        // Arrangement leads with the chorus, repeats it; each unique section is
        // written once, chorus first, and the unreferenced verse_2 comes last.
        let cho = to_chordpro(
            &s,
            &secs,
            &["chorus".into(), "verse_1".into(), "chorus".into()],
        );
        let chorus_at = cho.find("{start_of_chorus}").unwrap();
        let verse1_at = cho.find("{start_of_verse: Verse 1}").unwrap();
        let verse2_at = cho.find("{start_of_verse: Verse 2}").unwrap();
        assert!(chorus_at < verse1_at, "chorus should lead");
        assert!(verse1_at < verse2_at, "unreferenced verse_2 goes last");
        // A repeated arrangement entry does not duplicate the section body.
        assert_eq!(cho.matches("{start_of_chorus}").count(), 1);
    }

    #[test]
    fn chordpro_empty_song_is_just_directives() {
        let s = song("Empty", None, None);
        let cho = to_chordpro(&s, &[], &[]);
        assert!(cho.contains("{title: Empty}"));
        assert!(!cho.contains("{start_of_"));
    }

    // ── name mapping ─────────────────────────────────────────────────────────

    #[test]
    fn label_name_mapping_matches_importer_inverse() {
        assert_eq!(label_to_openlyrics_name("verse_1"), "v1");
        assert_eq!(label_to_openlyrics_name("chorus"), "c");
        assert_eq!(label_to_openlyrics_name("chorus_2"), "c2");
        assert_eq!(label_to_openlyrics_name("pre_chorus"), "p");
        assert_eq!(label_to_openlyrics_name("bridge"), "b");
        assert_eq!(label_to_openlyrics_name("tag"), "t");
        assert_eq!(label_to_openlyrics_name("ending"), "e");
        assert_eq!(label_to_openlyrics_name("instrumental"), "instrumental");
    }
}
