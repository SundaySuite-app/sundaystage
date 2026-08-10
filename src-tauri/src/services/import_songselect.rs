//! CCLI SongSelect song import (Spor B5).
//!
//! SongSelect (the paid CCLI lyric service) exports songs in two shapes, both
//! handled here and routed through the shared [`FormattedSong`] seam like every
//! other importer:
//!
//!   * **`.usr`** — an INI-like file: a `[S <ccli>]` section whose `Key=Value`
//!     lines carry the metadata, plus two parallel `/`-delimited lists,
//!     `Fields=` (the section labels) and `Words=` (the section bodies, one per
//!     field, lines within a body separated by real line breaks).
//!   * **`.txt`** — a plain-text layout: the title on the first line, then
//!     verse/chorus-labelled stanzas separated by blank lines, then a footer
//!     that begins `CCLI Song # <n>`, lists the author(s) and copyright, and
//!     ends `CCLI License # <n>`.
//!
//! The two carriers are told apart by content (`.usr` always has a `Words=`
//! line) rather than by extension, because [`parse_songselect`] only receives
//! the file's text. Metadata that [`FormattedSong`] cannot hold (the CCLI song
//! id and the copyright/author credit) travels the [`ImportMetadata`] seam that
//! B4 added, applied to the song row by the import command.
//!
//! The `.usr`/`.txt` structure is a documented, observable format. Praisenter
//! (BSD-3-Clause) has a reference reader for it; the shape below was
//! reimplemented from the format spec, not copied from its Java. Attribution is
//! recorded in `THIRD-PARTY.md`.

use crate::services::ai::lyric_format::{detect_header, FormattedSong};
use crate::services::song_import::{finalize, Block, ImportMetadata};

/// Keys recognised inside a `.usr` `[S …]` section (lower-cased match). Anything
/// else on its own line is treated as a continuation of the previous value —
/// which is how a multi-line `Words=` body is stitched back together.
const USR_KEYS: &[&str] = &[
    "title",
    "author",
    "copyright",
    "admin",
    "keys",
    "themes",
    "fields",
    "words",
    "type",
    "version",
    "ccli",
];

/// Parse a SongSelect `.usr` or `.txt` export into a [`FormattedSong`].
pub fn parse_songselect(content: &str) -> FormattedSong {
    if is_usr(content) {
        parse_usr(content)
    } else {
        parse_txt(content)
    }
}

/// Extract the CCLI id + copyright/author credit that live outside
/// [`FormattedSong`]. A cheap, independent scan (matching the `import_song_file`
/// seam) rather than threading it back out of the parse.
pub fn extract_metadata(content: &str) -> ImportMetadata {
    if is_usr(content) {
        usr_metadata(content)
    } else {
        txt_metadata(content)
    }
}

/// `.usr` files always carry a `Words=` line; a `.txt` export never does. That
/// single marker discriminates the two carriers from content alone.
fn is_usr(content: &str) -> bool {
    content
        .lines()
        .any(|l| l.trim_start().to_lowercase().starts_with("words="))
}

// ── .usr (INI-like) ─────────────────────────────────────────────────────────

/// Collect the `Key=Value` pairs of a `.usr`, honouring multi-line values: a
/// line that is neither a `[section]` nor a known `Key=` continues the current
/// value on a new line (so a `Words=` body with real line breaks survives).
fn usr_fields(content: &str) -> std::collections::HashMap<String, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current: Option<String> = None;
    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current = None; // a section header ends the previous value
            continue;
        }
        if let Some((key, value)) = raw.split_once('=') {
            let key_norm = key.trim().to_lowercase();
            if USR_KEYS.contains(&key_norm.as_str()) {
                map.insert(key_norm.clone(), value.to_string());
                current = Some(key_norm);
                continue;
            }
        }
        // Continuation of the current value (e.g. a lyric line inside Words=).
        if let Some(key) = &current {
            let entry = map.entry(key.clone()).or_default();
            entry.push('\n');
            entry.push_str(raw);
        }
    }
    map
}

fn parse_usr(content: &str) -> FormattedSong {
    let map = usr_fields(content);
    let title = map
        .get("title")
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    let fields: Vec<String> = match map.get("fields") {
        Some(f) => f.split('/').map(|s| s.trim().to_string()).collect(),
        None => Vec::new(),
    };
    // Section bodies are `/`-delimited; keep internal line breaks intact.
    let bodies: Vec<String> = match map.get("words") {
        Some(w) => w
            .split('/')
            .map(|s| s.trim_matches('\n').to_string())
            .collect(),
        None => Vec::new(),
    };

    let mut raw_text = String::new();
    let mut blocks: Vec<Block> = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        let lines: Vec<String> = body.lines().map(|l| l.trim_end().to_string()).collect();
        for l in &lines {
            raw_text.push_str(l);
            raw_text.push('\n');
        }
        // The i-th field labels the i-th body; an unmatched body auto-numbers.
        let label = fields
            .get(i)
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty());
        blocks.push((label, lines));
    }

    finalize(title, &raw_text, blocks)
}

fn usr_metadata(content: &str) -> ImportMetadata {
    let map = usr_fields(content);
    // CCLI id: a dedicated `CCLI=` key wins, else the digits of the `[S …]`
    // section header (e.g. `[S A22025]` → 22025).
    let ccli_song_id = map
        .get("ccli")
        .and_then(|v| extract_ccli_number(v))
        .or_else(|| section_ccli(content));
    let author = map.get("author").map(|s| s.trim()).unwrap_or("");
    let copyright = map.get("copyright").map(|s| s.trim()).unwrap_or("");
    ImportMetadata {
        ccli_song_id,
        copyright_notice: credit(author, copyright),
    }
}

/// The CCLI number from a `.usr` `[S <id>]` section header, if any.
fn section_ccli(content: &str) -> Option<String> {
    for raw in content.lines() {
        let t = raw.trim();
        if t.starts_with("[S") && t.ends_with(']') {
            let inner = &t[1..t.len() - 1]; // strip [ ]
            if let Some(n) = extract_ccli_number(inner) {
                return Some(n);
            }
        }
    }
    None
}

// ── .txt (plain-text layout) ────────────────────────────────────────────────

/// The line index where the CCLI footer begins (`CCLI Song #`), if present.
fn txt_footer_start(lines: &[&str]) -> Option<usize> {
    lines.iter().position(|l| l.contains("CCLI Song #"))
}

fn parse_txt(content: &str) -> FormattedSong {
    let lines: Vec<&str> = content.lines().collect();

    // Title = the first non-empty line.
    let title_idx = lines.iter().position(|l| !l.trim().is_empty());
    let title = title_idx
        .map(|i| lines[i].trim().to_string())
        .filter(|t| !t.is_empty());

    // Body = between the title and the CCLI footer (or EOF).
    let body_start = title_idx.map(|i| i + 1).unwrap_or(0);
    let body_end = txt_footer_start(&lines).unwrap_or(lines.len());
    let body_end = body_end.max(body_start);
    let body = &lines[body_start..body_end];

    // Split the body into blank-line-delimited stanzas; the first line of a
    // stanza is its label only when it reads as a section header.
    let mut raw_text = String::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut stanza: Vec<String> = Vec::new();
    let flush = |stanza: &mut Vec<String>, blocks: &mut Vec<Block>, raw_text: &mut String| {
        if stanza.is_empty() {
            return;
        }
        let mut lines: Vec<String> = std::mem::take(stanza);
        let label = detect_header(&lines[0]);
        if label.is_some() {
            lines.remove(0); // the header line is not a lyric
        }
        for l in &lines {
            raw_text.push_str(l);
            raw_text.push('\n');
        }
        blocks.push((label, lines));
    };
    for line in body {
        if line.trim().is_empty() {
            flush(&mut stanza, &mut blocks, &mut raw_text);
        } else {
            stanza.push(line.trim_end().to_string());
        }
    }
    flush(&mut stanza, &mut blocks, &mut raw_text);

    finalize(title, &raw_text, blocks)
}

fn txt_metadata(content: &str) -> ImportMetadata {
    let lines: Vec<&str> = content.lines().collect();
    let footer = match txt_footer_start(&lines) {
        Some(i) => i,
        None => return ImportMetadata::default(),
    };
    let ccli_song_id = extract_ccli_number(lines[footer]);

    // Credit lines: everything after `CCLI Song #`, minus the boilerplate terms
    // line and the `CCLI License #` line — i.e. the author(s) and copyright.
    let credit: Vec<String> = lines[footer + 1..]
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| !l.contains("CCLI License #"))
        .filter(|l| !l.contains("SongSelect") && !l.to_lowercase().contains("all rights reserved"))
        .map(str::to_string)
        .collect();
    let copyright_notice = (!credit.is_empty()).then(|| credit.join("\n"));

    ImportMetadata {
        ccli_song_id,
        copyright_notice,
    }
}

// ── shared helpers ──────────────────────────────────────────────────────────

/// The first contiguous run of ASCII digits in `s`, as an owned string.
/// `"CCLI Song # 4768151"` → `Some("4768151")`; `"S A22025"` → `Some("22025")`.
fn extract_ccli_number(s: &str) -> Option<String> {
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// Fold an author line and a copyright line into the single copyright-notice
/// field we have room for (both formats lack a dedicated author column). Empty
/// parts are dropped; all-empty yields `None`.
fn credit(author: &str, copyright: &str) -> Option<String> {
    let parts: Vec<&str> = [author, copyright]
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::song_import::{import_song, ImportFormat};

    fn labels(song: &FormattedSong) -> Vec<&str> {
        song.sections.iter().map(|s| s.label.as_str()).collect()
    }

    // ── .usr ─────────────────────────────────────────────────────────────────

    #[test]
    fn usr_parses_fields_words_and_metadata() {
        let src = "[File]\n\
                   Type=SongSelect Import File\n\
                   Version=3.0\n\
                   [S A22025]\n\
                   Title=Amazing Grace\n\
                   Author=John Newton\n\
                   Copyright=Public Domain\n\
                   Keys=G\n\
                   Fields=Verse 1/Chorus/Verse 2\n\
                   Words=Amazing grace how sweet the sound\n\
                   That saved a wretch like me/My chains are gone\n\
                   I've been set free/Twas grace that taught my heart to fear";
        let song = parse_songselect(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Amazing Grace"));
        assert_eq!(labels(&song), vec!["verse_1", "chorus", "verse_2"]);
        assert_eq!(
            song.sections[0].lyrics,
            "Amazing grace how sweet the sound\nThat saved a wretch like me"
        );
        assert_eq!(
            song.sections[1].lyrics,
            "My chains are gone\nI've been set free"
        );
        assert_eq!(song.arrangement, vec!["verse_1", "chorus", "verse_2"]);

        let meta = extract_metadata(src);
        assert_eq!(meta.ccli_song_id.as_deref(), Some("22025"));
        assert_eq!(
            meta.copyright_notice.as_deref(),
            Some("John Newton\nPublic Domain")
        );
    }

    #[test]
    fn usr_handles_norwegian_characters() {
        let src = "[S A7161097]\n\
                   Title=Stor Er Din Trofasthet\n\
                   Author=Thomas Chisholm\n\
                   Copyright=© 1923 Hope Publishing\n\
                   Fields=Vers 1/Refreng\n\
                   Words=Stor er din trofasthet, å Gud, min Far\n\
                   Det er kje skiftande skugge hjå deg/Å, kor stor er din trofasthet mot meg";
        let song = parse_songselect(src);
        assert_eq!(
            song.title_suggestion.as_deref(),
            Some("Stor Er Din Trofasthet")
        );
        // "Vers 1" → verse_1, "Refreng" → chorus (shared canonicalisation).
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert!(song.sections[0].lyrics.contains("å Gud, min Far"));
        assert!(song.sections[1].lyrics.contains("Å, kor stor"));

        let meta = extract_metadata(src);
        assert_eq!(meta.ccli_song_id.as_deref(), Some("7161097"));
        assert!(meta
            .copyright_notice
            .as_deref()
            .unwrap()
            .contains("© 1923 Hope Publishing"));
    }

    #[test]
    fn usr_routes_through_full_import_pipeline() {
        // Extension-based detection + parse in one call.
        let src = "[S A22025]\nTitle=Grace\nFields=Verse 1\nWords=Line one\nLine two";
        let (fmt, song) = import_song("amazing.usr", src);
        assert_eq!(fmt, ImportFormat::SongSelect);
        assert_eq!(song.title_suggestion.as_deref(), Some("Grace"));
        assert_eq!(song.sections[0].lyrics, "Line one\nLine two");
    }

    // ── .txt ─────────────────────────────────────────────────────────────────

    #[test]
    fn txt_parses_title_stanzas_and_footer() {
        let src = "Amazing Grace (My Chains Are Gone)\n\
                   \n\
                   Verse 1\n\
                   Amazing grace how sweet the sound\n\
                   That saved a wretch like me\n\
                   \n\
                   Chorus\n\
                   My chains are gone, I've been set free\n\
                   \n\
                   Verse 2\n\
                   Twas grace that taught my heart to fear\n\
                   \n\
                   CCLI Song # 4768151\n\
                   Chris Tomlin | John Newton | Louie Giglio\n\
                   © 2006 sixsteps Music\n\
                   \n\
                   For use solely with the SongSelect Terms of Use. All rights reserved. www.ccli.com\n\
                   \n\
                   CCLI License # 1234567";
        let song = parse_songselect(src);
        assert_eq!(
            song.title_suggestion.as_deref(),
            Some("Amazing Grace (My Chains Are Gone)")
        );
        assert_eq!(labels(&song), vec!["verse_1", "chorus", "verse_2"]);
        assert_eq!(
            song.sections[0].lyrics,
            "Amazing grace how sweet the sound\nThat saved a wretch like me"
        );
        assert_eq!(
            song.sections[1].lyrics,
            "My chains are gone, I've been set free"
        );

        let meta = extract_metadata(src);
        assert_eq!(meta.ccli_song_id.as_deref(), Some("4768151"));
        let cr = meta.copyright_notice.unwrap();
        assert!(cr.contains("Chris Tomlin | John Newton | Louie Giglio"));
        assert!(cr.contains("© 2006 sixsteps Music"));
        // Boilerplate terms + the license line are NOT part of the credit.
        assert!(!cr.contains("SongSelect"));
        assert!(!cr.contains("CCLI License #"));
    }

    #[test]
    fn txt_handles_norwegian_and_ccli_signature_detection() {
        let src = "Deg Å Få Skoda\n\
                   \n\
                   Vers 1\n\
                   Deg å få skoda er sæla å nå\n\
                   \n\
                   Refreng\n\
                   Æ, Ø og Å står støtt\n\
                   \n\
                   CCLI Song # 5148449\n\
                   Anders Hovden\n\
                   CCLI License # 42";
        let (fmt, song) = import_song("deg.txt", src);
        assert_eq!(fmt, ImportFormat::SongSelect);
        assert_eq!(song.title_suggestion.as_deref(), Some("Deg Å Få Skoda"));
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert!(song.sections[1].lyrics.contains("Æ, Ø og Å"));
        assert_eq!(
            extract_metadata(src).ccli_song_id.as_deref(),
            Some("5148449")
        );
    }

    #[test]
    fn txt_without_labels_auto_numbers_verses() {
        let src = "Untitled Hymn\n\
                   \n\
                   First stanza line one\n\
                   first stanza line two\n\
                   \n\
                   Second stanza line one\n\
                   \n\
                   CCLI Song # 999\n\
                   CCLI License # 1";
        let song = parse_songselect(src);
        assert_eq!(labels(&song), vec!["verse_1", "verse_2"]);
    }

    // ── robustness ─────────────────────────────────────────────────────────────

    #[test]
    fn empty_and_malformed_degrade_gracefully() {
        for src in [
            "",
            "Just a title with nothing else",
            "[S A1]\nTitle=Only metadata\n", // .usr with no Words → txt path, no body
            "CCLI Song # 5\nCCLI License # 9", // footer only, no lyrics
        ] {
            let song = parse_songselect(src);
            // Never panics; a song with no sections carries a warning.
            if song.sections.is_empty() {
                assert!(!song.warnings.is_empty(), "{src:?} should warn");
            }
            let _ = extract_metadata(src);
        }
    }

    #[test]
    fn extract_ccli_number_takes_first_digit_run() {
        assert_eq!(
            extract_ccli_number("CCLI Song # 4768151").as_deref(),
            Some("4768151")
        );
        assert_eq!(extract_ccli_number("S A22025").as_deref(), Some("22025"));
        assert_eq!(extract_ccli_number("no digits here"), None);
    }
}
