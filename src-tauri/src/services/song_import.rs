//! Phase 2.2 — song import.
//!
//! Pure parsers that turn song files exported from other worship apps into a
//! [`FormattedSong`] — the same structure the AI / heuristic lyric formatter
//! produces — so the existing `apply_formatted_song` path inserts them with no
//! special-casing. Everything in this module is pure and fixture-tested; the
//! Tauri command that reads file content and creates the song lives in
//! `commands::import`.
//!
//! Supported, dependency-free text/XML formats:
//!   * **Plain text** — delegates to the heuristic formatter.
//!   * **ChordPro** (`.cho`/`.crd`/`.chopro`/`.chordpro`) — `{directives}`
//!     plus inline `[chords]`.
//!   * **OpenSong** — `<song><lyrics>` with the `.`/` `/`[V1]` mini-format.
//!   * **OpenLyrics** (OpenLP `.xml`) — `<verse name="v1"><lines>…</lines>`.
//!
//! Binary / proprietary formats (ProPresenter `.pro`, EasyWorship, FreeShow
//! `.show`) need format-specific decoders and are intentionally out of scope.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::ai::lyric_format::{
    detect_header, detect_language, heuristic_format, normalize_label, unique_label,
    FormattedSection, FormattedSong,
};

/// The file formats the importer recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ImportFormat.ts")]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    PlainText,
    ChordPro,
    OpenSong,
    OpenLyrics,
}

/// Best-effort format detection from a filename and the content itself. Content
/// signatures win over the extension (a `.txt` holding ChordPro directives is
/// ChordPro). Falls back to plain text.
pub fn detect_format(filename: &str, content: &str) -> ImportFormat {
    let name = filename.to_lowercase();
    let head = content.trim_start();

    // OpenLyrics is the most specific XML: a namespace marker or named verses.
    if head.starts_with('<') && (content.contains("openlyrics") || content.contains("<verse name"))
    {
        return ImportFormat::OpenLyrics;
    }
    // OpenSong: an XML document carrying a <lyrics> element.
    if head.starts_with('<') && content.contains("<lyrics") {
        return ImportFormat::OpenSong;
    }
    // ChordPro: extension or a tell-tale directive. `.pro` is deliberately NOT
    // mapped here — ProPresenter uses it for a binary format.
    let chordpro_ext = [".cho", ".crd", ".chopro", ".chordpro", ".pro_"]
        .iter()
        .any(|e| name.ends_with(e));
    let chordpro_directive = ["{title:", "{t:", "{start_of_", "{soc}", "{sov}", "{c:"]
        .iter()
        .any(|d| content.contains(d));
    if chordpro_ext || chordpro_directive {
        return ImportFormat::ChordPro;
    }
    if name.ends_with(".xml") && content.contains("<verse") {
        return ImportFormat::OpenLyrics;
    }
    ImportFormat::PlainText
}

/// Parse `content` according to `format` into a [`FormattedSong`].
pub fn parse_song(content: &str, format: ImportFormat) -> FormattedSong {
    match format {
        ImportFormat::PlainText => heuristic_format(content),
        ImportFormat::ChordPro => parse_chordpro(content),
        ImportFormat::OpenSong => parse_opensong(content),
        ImportFormat::OpenLyrics => parse_openlyrics(content),
    }
}

/// Detect the format from filename+content and parse in one call.
pub fn import_song(filename: &str, content: &str) -> (ImportFormat, FormattedSong) {
    let format = detect_format(filename, content);
    (format, parse_song(content, format))
}

// ── Shared assembly ─────────────────────────────────────────────────────────────

/// A parsed block before label numbering/dedup: an optional explicit label and
/// its lyric lines.
type Block = (Option<String>, Vec<String>);

/// Resolve labels (auto-numbering verses), dedup identical lyric blocks into one
/// section referenced multiple times in the arrangement, and package the result.
fn finalize(title: Option<String>, raw_text: &str, blocks: Vec<Block>) -> FormattedSong {
    let mut verse_n = 0usize;
    let mut ordered: Vec<(String, String)> = Vec::new();

    for (label, lines) in blocks {
        let lyrics = lines.join("\n");
        if lyrics.trim().is_empty() {
            continue;
        }
        let resolved = match label {
            Some(l) => {
                let norm = normalize_label(&l);
                if norm == "verse" {
                    verse_n += 1;
                    format!("verse_{verse_n}")
                } else {
                    // Keep explicit numbering in sync so later auto-verses don't collide.
                    if let Some(rest) = norm.strip_prefix("verse_") {
                        if let Ok(k) = rest.parse::<usize>() {
                            verse_n = verse_n.max(k);
                        }
                    }
                    norm
                }
            }
            None => {
                verse_n += 1;
                format!("verse_{verse_n}")
            }
        };
        ordered.push((resolved, lyrics));
    }

    assemble(title, detect_language(raw_text), ordered)
}

/// Build the final [`FormattedSong`] from ordered (label, lyrics) pairs: dedup
/// identical lyrics to one section, keep the play order as the arrangement.
fn assemble(
    title: Option<String>,
    language: String,
    ordered: Vec<(String, String)>,
) -> FormattedSong {
    use std::collections::HashMap;

    let mut sections: Vec<FormattedSection> = Vec::new();
    let mut arrangement: Vec<String> = Vec::new();
    let mut by_lyrics: HashMap<String, String> = HashMap::new();

    for (label, lyrics) in ordered {
        let lyrics = lyrics.trim_matches('\n').trim_end().to_string();
        if lyrics.trim().is_empty() {
            continue;
        }
        if let Some(existing) = by_lyrics.get(&lyrics) {
            arrangement.push(existing.clone());
            continue;
        }
        let label = unique_label(&sections, label);
        by_lyrics.insert(lyrics.clone(), label.clone());
        sections.push(FormattedSection {
            label: label.clone(),
            lyrics,
        });
        arrangement.push(label);
    }

    let mut warnings = Vec::new();
    if sections.is_empty() {
        warnings.push("Fant ingen sang-seksjoner i filen.".to_string());
    }

    FormattedSong {
        title_suggestion: title.and_then(|t| {
            let t = t.trim().to_string();
            (!t.is_empty()).then_some(t)
        }),
        language,
        sections,
        arrangement,
        warnings,
    }
}

/// Map a short section code (`V1`, `c`, `b2`, `p`, `o`) to a canonical label.
/// Used by both OpenSong (`[V1]`) and OpenLyrics (`name="v1"`), case-insensitive.
fn code_to_label(code: &str) -> String {
    let t = code.trim();
    let alpha: String = t.chars().take_while(|c| c.is_alphabetic()).collect();
    let num: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    let word = match alpha.to_lowercase().as_str() {
        "v" => "verse",
        "c" => "chorus",
        "b" => "bridge",
        "p" => "pre_chorus",
        "t" => "tag",
        "i" => "intro",
        "e" | "o" => "ending",
        _ => return normalize_label(t),
    };
    if num.is_empty() {
        normalize_label(word)
    } else {
        normalize_label(&format!("{word} {num}"))
    }
}

// ── ChordPro ──────────────────────────────────────────────────────────────────

enum Directive {
    Title(String),
    StartOf(String),
    EndOf,
    Comment(String),
    Other,
}

fn parse_directive(line: &str) -> Option<Directive> {
    let t = line.trim();
    if !(t.starts_with('{') && t.ends_with('}')) {
        return None;
    }
    let inner = &t[1..t.len() - 1];
    let (name, value) = match inner.split_once(':') {
        Some((n, v)) => (n.trim().to_lowercase(), v.trim().to_string()),
        None => (inner.trim().to_lowercase(), String::new()),
    };
    let dir = match name.as_str() {
        "title" | "t" => Directive::Title(value),
        "start_of_verse" | "sov" => Directive::StartOf("verse".into()),
        "start_of_chorus" | "soc" => Directive::StartOf("chorus".into()),
        "start_of_bridge" | "sob" => Directive::StartOf("bridge".into()),
        "end_of_verse" | "eov" | "end_of_chorus" | "eoc" | "end_of_bridge" | "eob" => {
            Directive::EndOf
        }
        "comment" | "c" | "ci" | "comment_italic" => Directive::Comment(value),
        _ => Directive::Other,
    };
    Some(dir)
}

/// Remove inline `[chord]` markers, keeping the lyric text.
fn strip_inline_chords(line: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in line.chars() {
        match c {
            '[' => depth += 1,
            ']' => {
                if depth > 0 {
                    depth = depth.saturating_sub(1);
                } else {
                    out.push(c);
                }
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim_end().to_string()
}

fn parse_chordpro(content: &str) -> FormattedSong {
    let mut title: Option<String> = None;
    let mut blocks: Vec<Block> = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut raw_text = String::new();

    let flush = |label: &mut Option<String>, lines: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !lines.is_empty() {
            blocks.push((label.clone(), std::mem::take(lines)));
        }
    };

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush(&mut current_label, &mut current_lines, &mut blocks);
            current_label = None;
            continue;
        }
        if let Some(dir) = parse_directive(line) {
            match dir {
                Directive::Title(t) => title = Some(t),
                Directive::StartOf(label) => {
                    flush(&mut current_label, &mut current_lines, &mut blocks);
                    current_label = Some(label);
                }
                Directive::EndOf => {
                    flush(&mut current_label, &mut current_lines, &mut blocks);
                    current_label = None;
                }
                Directive::Comment(c) => {
                    if let Some(label) = detect_header(&c) {
                        flush(&mut current_label, &mut current_lines, &mut blocks);
                        current_label = Some(label);
                    }
                }
                Directive::Other => {}
            }
            continue;
        }
        let stripped = strip_inline_chords(line);
        if stripped.trim().is_empty() {
            continue; // chord-only line
        }
        raw_text.push_str(&stripped);
        raw_text.push('\n');
        current_lines.push(stripped);
    }
    flush(&mut current_label, &mut current_lines, &mut blocks);

    finalize(title, &raw_text, blocks)
}

// ── OpenSong ────────────────────────────────────────────────────────────────────

fn parse_opensong(content: &str) -> FormattedSong {
    let title = extract_element(content, "title");
    let lyrics = extract_element(content, "lyrics").unwrap_or_default();

    let mut blocks: Vec<Block> = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut raw_text = String::new();
    let mut saw_group_digits = false;

    for raw_line in lyrics.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        // Section marker: [V1], [C], [Bridge] …
        if trimmed.starts_with('[') {
            if let Some(end) = trimmed.find(']') {
                if !current_lines.is_empty() {
                    blocks.push((current_label.clone(), std::mem::take(&mut current_lines)));
                }
                let tag = &trimmed[1..end];
                current_label = Some(code_to_label(tag));
                continue;
            }
        }
        // Chord line / comment line.
        if trimmed.starts_with('.') || trimmed.starts_with(';') {
            continue;
        }
        // Lyric line: drop a single leading space, note verse-group digits.
        let mut text = line.strip_prefix(' ').unwrap_or(line).to_string();
        if let Some(rest) = text.strip_prefix(|c: char| c.is_ascii_digit()) {
            if rest.starts_with(' ') {
                saw_group_digits = true;
            }
        }
        text = text.trim_end().to_string();
        if text.trim().is_empty() {
            continue;
        }
        raw_text.push_str(&text);
        raw_text.push('\n');
        current_lines.push(text);
    }
    if !current_lines.is_empty() {
        blocks.push((current_label.clone(), current_lines));
    }

    let mut song = finalize(title, &raw_text, blocks);
    if saw_group_digits {
        song.warnings.push(
            "OpenSong vers-gruppering (tall-kolonner) ble ikke tolket — sjekk versinndelingen."
                .to_string(),
        );
    }
    song
}

// ── OpenLyrics ──────────────────────────────────────────────────────────────────

fn parse_openlyrics(content: &str) -> FormattedSong {
    let title = extract_element(content, "title");
    let verse_order = extract_element(content, "verseOrder");

    // Collect verses in document order: (name, lyrics).
    let verses = openlyrics_verses(content);

    let mut raw_text = String::new();
    for (_, lyrics) in &verses {
        raw_text.push_str(lyrics);
        raw_text.push('\n');
    }

    let mut warnings = Vec::new();
    let ordered_names: Vec<String> = match &verse_order {
        Some(order) if !order.trim().is_empty() => {
            order.split_whitespace().map(|s| s.to_string()).collect()
        }
        _ => verses.iter().map(|(name, _)| name.clone()).collect(),
    };

    let mut blocks: Vec<Block> = Vec::new();
    for name in &ordered_names {
        match verses.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some((_, lyrics)) => {
                let lines: Vec<String> = lyrics.lines().map(|l| l.to_string()).collect();
                blocks.push((Some(code_to_label(name)), lines));
            }
            None => warnings.push(format!("Arrangement viste til ukjent vers «{name}».")),
        }
    }

    // Append any verse present in the file but never referenced by verseOrder.
    // verseOrder is a *presentation* order, not a content filter (OpenLP and
    // ProPresenter keep every verse): silently dropping these would lose lyrics
    // on import. They go after the ordered blocks so the explicit play order is
    // preserved, and we flag them so the operator can re-sequence if desired.
    for (name, lyrics) in &verses {
        let referenced = ordered_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name));
        if !referenced {
            let lines: Vec<String> = lyrics.lines().map(|l| l.to_string()).collect();
            blocks.push((Some(code_to_label(name)), lines));
            if !name.trim().is_empty() {
                warnings.push(format!(
                    "Vers «{name}» manglet i arrangementet og ble lagt til til slutt."
                ));
            }
        }
    }

    let mut song = finalize(title, &raw_text, blocks);
    song.warnings.extend(warnings);
    song
}

/// Extract each `<verse name="…">` block's text (joining multiple `<lines>`),
/// in document order.
fn openlyrics_verses(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = content[search..].find("<verse") {
        let pos = search + rel;
        // Require a tag-name boundary so `<verseOrder>` is not treated as a verse.
        let after = pos + "<verse".len();
        let next = content[after..].chars().next();
        if !matches!(
            next,
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some('/')
        ) {
            search = after;
            continue;
        }
        let gt = match content[pos..].find('>') {
            Some(g) => pos + g,
            None => break,
        };
        let open_tag = &content[pos..=gt];
        let name = attr_value(open_tag, "name").unwrap_or_default();
        let close = "</verse>";
        let end = match content[gt + 1..].find(close) {
            Some(e) => gt + 1 + e,
            None => break,
        };
        let inner = &content[gt + 1..end];

        // Each <lines>…</lines> is a slide; join with newlines.
        let mut parts: Vec<String> = Vec::new();
        let mut s2 = 0usize;
        while let Some(lrel) = inner[s2..].find("<lines") {
            let lpos = s2 + lrel;
            let lgt = match inner[lpos..].find('>') {
                Some(g) => lpos + g,
                None => break,
            };
            let lclose = "</lines>";
            let lend = match inner[lgt + 1..].find(lclose) {
                Some(e) => lgt + 1 + e,
                None => break,
            };
            parts.push(openlyrics_lines_text(&inner[lgt + 1..lend]));
            s2 = lend + lclose.len();
        }
        let lyrics = if parts.is_empty() {
            openlyrics_lines_text(inner)
        } else {
            parts.join("\n")
        };
        out.push((name, lyrics));
        search = end + close.len();
    }
    out
}

/// Turn an OpenLyrics `<lines>` body into plain text: `<br/>` → newline, drop
/// all other tags (chords, comments), decode entities.
fn openlyrics_lines_text(inner: &str) -> String {
    let with_nl = inner
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");
    let no_tags = strip_xml_tags(&with_nl);
    let decoded = decode_entities(&no_tags);
    decoded
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

// ── Tiny XML helpers (dependency-free, scoped to these well-known schemas) ──────

/// Inner text of the first `<tag>…</tag>` element, matched on a tag-name
/// boundary so `<title>` is not confused with `<titles>`. CDATA-aware, entities
/// decoded. Returns `None` if absent or self-closing/empty.
fn extract_element(xml: &str, tag: &str) -> Option<String> {
    let needle = format!("<{tag}");
    let mut search = 0usize;
    while let Some(rel) = xml[search..].find(&needle) {
        let pos = search + rel;
        let after = pos + needle.len();
        let next = xml[after..].chars().next();
        let boundary = matches!(
            next,
            Some('>') | Some(' ') | Some('/') | Some('\t') | Some('\n') | Some('\r')
        );
        if !boundary {
            search = after;
            continue;
        }
        let gt = xml[pos..].find('>')? + pos;
        if xml.as_bytes().get(gt.wrapping_sub(1)) == Some(&b'/') {
            return None; // self-closing
        }
        let inner_start = gt + 1;
        let close = format!("</{tag}>");
        let end_rel = xml[inner_start..].find(&close)?;
        let inner = &xml[inner_start..inner_start + end_rel];
        let unwrapped = unwrap_cdata(inner);
        let text = decode_entities(&strip_xml_tags(&unwrapped));
        return Some(text.trim().to_string());
    }
    None
}

/// Read an attribute value (`name="…"` or `name='…'`) from an opening tag.
fn attr_value(open_tag: &str, attr: &str) -> Option<String> {
    let key = format!("{attr}=");
    let start = open_tag.find(&key)? + key.len();
    let rest = &open_tag[start..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = &rest[1..];
    let end = body.find(quote)?;
    Some(decode_entities(&body[..end]))
}

fn unwrap_cdata(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("<![CDATA[") {
        if let Some(inner) = rest.strip_suffix("]]>") {
            return inner.to_string();
        }
    }
    s.to_string()
}

/// Remove every `<…>` tag, keeping the text between them.
fn strip_xml_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(song: &FormattedSong) -> Vec<&str> {
        song.sections.iter().map(|s| s.label.as_str()).collect()
    }

    // ── detection ──────────────────────────────────────────────────────────────

    #[test]
    fn detects_formats() {
        assert_eq!(
            detect_format(
                "song.xml",
                "<song xmlns=\"http://openlyrics.info\"><verse name=\"v1\">"
            ),
            ImportFormat::OpenLyrics
        );
        assert_eq!(
            detect_format(
                "song.xml",
                "<song><title>X</title><lyrics>[V1]\n hi</lyrics></song>"
            ),
            ImportFormat::OpenSong
        );
        assert_eq!(
            detect_format("song.txt", "{title: Hi}\n{soc}\nLine"),
            ImportFormat::ChordPro
        );
        assert_eq!(detect_format("song.cho", "Line"), ImportFormat::ChordPro);
        assert_eq!(
            detect_format("notes.txt", "Verse 1\nHello"),
            ImportFormat::PlainText
        );
    }

    // ── ChordPro ─────────────────────────────────────────────────────────────────

    #[test]
    fn chordpro_strips_chords_and_uses_environments() {
        let src = "{title: Amazing Grace}\n\
                   {start_of_verse}\n\
                   [G]Amazing [C]grace how [G]sweet the sound\n\
                   That [D]saved a wretch like [G]me\n\
                   {end_of_verse}\n\
                   {start_of_chorus}\n\
                   Praise [C]God\n\
                   {end_of_chorus}";
        let song = parse_chordpro(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Amazing Grace"));
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert_eq!(
            song.sections[0].lyrics.lines().next(),
            Some("Amazing grace how sweet the sound")
        );
        assert!(!song.sections[0].lyrics.contains('['));
        assert_eq!(song.arrangement, vec!["verse_1", "chorus"]);
    }

    #[test]
    fn chordpro_comment_acts_as_section_header() {
        let src = "{c: Verse 1}\nLine one\n\n{comment: Chorus}\nHook line";
        let song = parse_chordpro(src);
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
    }

    #[test]
    fn chordpro_blank_lines_split_unlabelled_verses() {
        let src = "First verse line\nsecond line\n\nSecond verse line\nmore";
        let song = parse_chordpro(src);
        assert_eq!(labels(&song), vec!["verse_1", "verse_2"]);
    }

    // ── OpenSong ─────────────────────────────────────────────────────────────────

    #[test]
    fn opensong_parses_markers_chords_and_title() {
        let src = "<song>\n\
                   <title>Be Thou My Vision</title>\n\
                   <author>Traditional</author>\n\
                   <lyrics>[V1]\n\
                   .G    C    G\n\
                   Be thou my vision\n\
                   O Lord of my heart\n\
                   [C]\n\
                   ;a comment\n\
                   Praise to the Lord\n\
                   </lyrics>\n\
                   </song>";
        let song = parse_opensong(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Be Thou My Vision"));
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert_eq!(
            song.sections[0].lyrics,
            "Be thou my vision\nO Lord of my heart"
        );
        assert!(!song.sections[0].lyrics.contains("G    C"));
    }

    #[test]
    fn opensong_decodes_entities_in_title() {
        let src = "<song><title>Holy &amp; Mighty</title><lyrics>[V1]\n Line</lyrics></song>";
        let song = parse_opensong(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Holy & Mighty"));
    }

    // ── OpenLyrics ───────────────────────────────────────────────────────────────

    #[test]
    fn openlyrics_parses_named_verses_and_br() {
        let src = r#"<?xml version="1.0"?>
<song xmlns="http://openlyrics.info/namespace/2009/song">
  <properties>
    <titles><title>Amazing Grace</title></titles>
  </properties>
  <lyrics>
    <verse name="v1"><lines>Amazing grace<br/>how sweet the sound</lines></verse>
    <verse name="c"><lines>Praise God</lines></verse>
  </lyrics>
</song>"#;
        let song = parse_openlyrics(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Amazing Grace"));
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert_eq!(
            song.sections[0].lyrics,
            "Amazing grace\nhow sweet the sound"
        );
    }

    #[test]
    fn openlyrics_title_not_confused_with_titles_wrapper() {
        let src = "<song><properties><titles><title>Real Title</title></titles></properties>\
                   <lyrics><verse name=\"v1\"><lines>x</lines></verse></lyrics></song>";
        let song = parse_openlyrics(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Real Title"));
    }

    #[test]
    fn openlyrics_honours_verse_order() {
        let src = "<song><properties><titles><title>T</title></titles>\
                   <verseOrder>v1 c v1</verseOrder></properties>\
                   <lyrics>\
                   <verse name=\"v1\"><lines>Verse words</lines></verse>\
                   <verse name=\"c\"><lines>Chorus words</lines></verse>\
                   </lyrics></song>";
        let song = parse_openlyrics(src);
        // v1 → c → v1 : two unique sections, arrangement repeats the verse.
        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert_eq!(song.arrangement, vec!["verse_1", "chorus", "verse_1"]);
    }

    #[test]
    fn openlyrics_keeps_verses_omitted_from_verse_order() {
        // verseOrder lists only v1, but the file also contains v2. OpenLyrics'
        // verseOrder is a presentation order, not a content filter — OpenLP and
        // ProPresenter keep every verse in the file. Dropping v2 entirely would
        // silently lose lyrics on import (data loss).
        let src = "<song><properties><titles><title>T</title></titles>\
                   <verseOrder>v1</verseOrder></properties>\
                   <lyrics>\
                   <verse name=\"v1\"><lines>First verse</lines></verse>\
                   <verse name=\"v2\"><lines>Second verse</lines></verse>\
                   </lyrics></song>";
        let song = parse_openlyrics(src);
        // Both verses must survive as sections…
        assert_eq!(labels(&song), vec!["verse_1", "verse_2"]);
        let v2 = song
            .sections
            .iter()
            .find(|s| s.label == "verse_2")
            .expect("v2 must not be lost");
        assert_eq!(v2.lyrics, "Second verse");
        // …and the explicit play order is still honoured (v1 first, the
        // unreferenced verse appended after so nothing vanishes).
        assert_eq!(song.arrangement.first().map(String::as_str), Some("verse_1"));
        assert!(song.arrangement.contains(&"verse_2".to_string()));
    }

    #[test]
    fn openlyrics_strips_chords_inside_lines() {
        let src = "<song><lyrics><verse name=\"v1\"><lines>\
                   <chord name=\"G\"/>Amazing grace</lines></verse></lyrics></song>";
        let song = parse_openlyrics(src);
        assert_eq!(song.sections[0].lyrics, "Amazing grace");
    }

    // ── empty / fallback ─────────────────────────────────────────────────────────

    #[test]
    fn empty_content_yields_warning_not_panic() {
        let song = parse_song("", ImportFormat::OpenLyrics);
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    #[test]
    fn plain_text_delegates_to_heuristic() {
        let song = parse_song(
            "Verse one\nline two\n\nVerse two\nline",
            ImportFormat::PlainText,
        );
        assert_eq!(song.sections.len(), 2);
    }

    // ── malformed input never panics; degrades to a warned stub ──────────────

    #[test]
    fn empty_content_for_every_format_warns_not_panics() {
        for fmt in [
            ImportFormat::PlainText,
            ImportFormat::ChordPro,
            ImportFormat::OpenSong,
            ImportFormat::OpenLyrics,
        ] {
            let song = parse_song("", fmt);
            assert!(song.sections.is_empty(), "{fmt:?} produced no sections");
            assert!(
                !song.warnings.is_empty(),
                "{fmt:?} warned about the empty file"
            );
        }
    }

    #[test]
    fn opensong_unclosed_lyrics_tag_degrades_gracefully() {
        // <lyrics> never closes — extract_element finds no </lyrics> and returns
        // None, so there's nothing to parse rather than a panic.
        let src = "<song><title>Broken</title><lyrics>[V1]\nA line that runs off";
        let song = parse_opensong(src);
        // Title was extracted before the break; body couldn't be read.
        assert_eq!(song.title_suggestion.as_deref(), Some("Broken"));
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    #[test]
    fn openlyrics_unclosed_verse_tag_degrades_gracefully() {
        // <verse> opens but never closes — openlyrics_verses breaks out instead
        // of slicing past the end of the string.
        let src = "<song><lyrics><verse name=\"v1\"><lines>Half a verse";
        let song = parse_openlyrics(src);
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    #[test]
    fn openlyrics_unclosed_lines_tag_degrades_gracefully() {
        // The verse closes but its <lines> does not.
        let src = "<song><lyrics><verse name=\"v1\"><lines>No closing tag</verse></lyrics></song>";
        let song = parse_openlyrics(src);
        // Falls back to the raw verse inner text rather than panicking.
        assert!(!song.warnings.is_empty() || !song.sections.is_empty());
    }

    #[test]
    fn detected_format_mismatching_its_tags_still_yields_a_stub() {
        // Claims OpenSong but carries no <lyrics> body — parser must not panic.
        let song = parse_song(
            "<song><title>Only A Title</title></song>",
            ImportFormat::OpenSong,
        );
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());

        // Claims OpenLyrics but has no verses at all.
        let song = parse_song("<song><properties/></song>", ImportFormat::OpenLyrics);
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    #[test]
    fn chordpro_with_only_directives_and_chords_yields_no_sections() {
        // Every lyric line is chord-only or a non-section directive: nothing to
        // show, but a clean warning instead of an empty-but-silent song.
        let src = "{title: Instrumental}\n{key: G}\n[G] [C] [D]\n[Em]";
        let song = parse_chordpro(src);
        assert_eq!(song.title_suggestion.as_deref(), Some("Instrumental"));
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    #[test]
    fn openlyrics_empty_and_whitespace_lines_bodies_are_dropped() {
        // Degenerate <lines> bodies (empty, whitespace, chords-only) must not
        // create empty sections.
        let src = "<song><lyrics>\
                   <verse name=\"v1\"><lines>   </lines></verse>\
                   <verse name=\"v2\"><lines><chord name=\"G\"/></lines></verse>\
                   <verse name=\"v3\"><lines>Real words here</lines></verse>\
                   </lyrics></song>";
        let song = parse_openlyrics(src);
        // Only the verse with actual words survives.
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].lyrics, "Real words here");
    }

    #[test]
    fn import_song_on_garbage_input_never_panics() {
        // Random bytes-as-text routed through full detect+parse: plain-text path,
        // degrades to a heuristic stub with a warning, no panic.
        let (fmt, song) = import_song("mystery.dat", "\u{0}\u{1}<<>>{{}}][");
        assert_eq!(fmt, ImportFormat::PlainText);
        // It produced *something* (stub or warned) without crashing.
        let _ = (song.sections.len(), song.warnings.len());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;

    // Deterministic LCG (fixed seed) — no external deps.
    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }
    }

    // Alphabet of bytes/fragments that exercise the parsers' structural paths,
    // including multibyte UTF-8 and the delimiter chars the slicers index on.
    const FRAGS: &[&str] = &[
        "<",
        ">",
        "<verse",
        "</verse>",
        "name=\"",
        "v1",
        "\"",
        "<lines>",
        "</lines>",
        "<br/>",
        "<title>",
        "</title>",
        "<lyrics>",
        "</lyrics>",
        "[V1]",
        "[C]",
        "[",
        "]",
        "{title:",
        "{soc}",
        "}",
        ".",
        ";",
        "\n",
        " ",
        "é",
        "ø",
        "Å",
        "字",
        "🎵",
        "&amp;",
        "&#39;",
        "<![CDATA[",
        "]]>",
        "verseOrder",
        "<verseOrder>",
        "openlyrics",
        "/",
        "\t",
        "\r",
        "G",
        "Am7",
        "0",
        "9",
        "chorus",
        "verse",
        "x2",
    ];

    fn random_input(rng: &mut Lcg) -> String {
        let n = rng.below(40);
        let mut s = String::new();
        for _ in 0..n {
            // 1/6 of the time inject a raw random byte as a char to stress UTF-8 paths.
            if rng.below(6) == 0 {
                let cp = rng.below(0x300) as u32;
                if let Some(c) = char::from_u32(cp) {
                    s.push(c);
                }
            } else {
                s.push_str(FRAGS[rng.below(FRAGS.len())]);
            }
        }
        s
    }

    #[test]
    fn fuzz_never_panics_and_arrangement_is_consistent() {
        let mut rng = Lcg(0x5151_4242_7373_9999);
        let fmts = [
            ImportFormat::PlainText,
            ImportFormat::ChordPro,
            ImportFormat::OpenSong,
            ImportFormat::OpenLyrics,
        ];
        for _ in 0..500 {
            let input = random_input(&mut rng);
            // detect + import end-to-end must not panic on arbitrary bytes.
            let (_fmt, song) = import_song("mystery.dat", &input);
            check_consistency(&song);
            // And each explicit format path independently.
            for fmt in fmts {
                let song = parse_song(&input, fmt);
                check_consistency(&song);
            }
        }
    }

    fn check_consistency(song: &FormattedSong) {
        use std::collections::HashSet;
        let labels: HashSet<&str> = song.sections.iter().map(|s| s.label.as_str()).collect();
        // INVARIANT: every arrangement entry references an existing section.
        for a in &song.arrangement {
            assert!(
                labels.contains(a.as_str()),
                "dangling arrangement ref {a:?}; labels={labels:?}"
            );
        }
        // INVARIANT: labels are unique.
        assert_eq!(
            labels.len(),
            song.sections.len(),
            "duplicate section labels: {:?}",
            song.sections.iter().map(|s| &s.label).collect::<Vec<_>>()
        );
        // INVARIANT: no section is empty (empty blocks are dropped).
        for s in &song.sections {
            assert!(
                !s.lyrics.trim().is_empty(),
                "empty section lyrics for {:?}",
                s.label
            );
        }
    }
}
