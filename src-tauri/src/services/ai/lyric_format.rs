//! Phase 4.2 — AI lyric formatting (the killer feature).
//!
//! Turns raw pasted lyrics (from CCLI, a YouTube description, a Norwegian
//! sangbok, …) into a structured song: labelled sections + a proposed
//! arrangement. Two paths share one output type [`FormattedSong`]:
//!
//!   * **AI** — a forced tool call to Claude returning structured JSON
//!     ([`system_prompt`], [`tool_schema`], [`parse_format_response`]).
//!   * **Heuristic** — [`heuristic_format`], a pure local formatter used when
//!     no API key is available (or the `ai` feature is off). It is the offline
//!     fallback the plan calls for and is exhaustively unit-tested.
//!
//! Everything here except [`apply_formatted_song`] is pure.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models::{SongArrangement, SongSection};
use crate::db::repositories::{ArrangementRepo, SongRepo};
use crate::error::{AppError, AppResult};
use sqlx::SqlitePool;

/// One labelled lyric block in a formatted song.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../src/lib/bindings/FormattedSection.ts")]
pub struct FormattedSection {
    /// Canonical snake_case label, e.g. `verse_1`, `chorus`, `pre_chorus`.
    pub label: String,
    /// Lyrics, newline-separated.
    pub lyrics: String,
}

/// The structured result of formatting raw lyrics.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export, export_to = "../../src/lib/bindings/FormattedSong.ts")]
pub struct FormattedSong {
    pub title_suggestion: Option<String>,
    /// ISO-639-1 language guess, e.g. `no`, `en`.
    pub language: String,
    pub sections: Vec<FormattedSection>,
    /// Ordered section labels (repeats allowed): verse_1 → chorus → verse_2 → …
    pub arrangement: Vec<String>,
    /// Notes for the user (dropped arrangement refs, low confidence, …).
    pub warnings: Vec<String>,
}

// ── AI path (prompt + schema + parsing) ────────────────────────────────────────

pub const TOOL_NAME: &str = "emit_formatted_song";

pub fn system_prompt() -> String {
    "You are a worship-song formatting assistant for church presentation \
software. Given raw, messy lyrics that may include chord lines, repetition \
markers (\"x2\", \"(repeat chorus)\"), and inconsistent spacing, produce a \
clean structured song.\n\n\
Rules:\n\
- Identify sections and label them: verse_1, verse_2, …, chorus, pre_chorus, \
bridge, intro, instrumental, tag, ending.\n\
- A repeated block (e.g. the chorus) is ONE section, referenced multiple times \
in the arrangement.\n\
- Remove chord-only lines and repetition markers from the lyrics themselves.\n\
- Preserve the original words and their order. Do not paraphrase or translate.\n\
- Detect the language (ISO-639-1).\n\
- Propose a sensible arrangement as an ordered list of section labels.\n\
- Call the emit_formatted_song tool with the result."
        .to_string()
}

/// JSON schema for the forced tool call.
pub fn tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "title_suggestion": { "type": ["string", "null"] },
            "language": { "type": "string" },
            "sections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string" },
                        "lyrics": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["label", "lyrics"]
                }
            },
            "arrangement": { "type": "array", "items": { "type": "string" } },
            "warnings": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["language", "sections", "arrangement"]
    })
}

/// Parse Claude's tool-call input (or any equivalent JSON) into a validated
/// [`FormattedSong`]. Normalizes labels, accepts `lyrics` as a string or an
/// array of lines, and drops arrangement references that don't match any
/// section (recording a warning) so the result is always internally consistent.
pub fn parse_format_response(input: &serde_json::Value) -> AppResult<FormattedSong> {
    let sections_json = input
        .get("sections")
        .and_then(|s| s.as_array())
        .ok_or_else(|| AppError::Internal("formatert sang mangler 'sections'".into()))?;

    let mut sections: Vec<FormattedSection> = Vec::new();
    for s in sections_json {
        let label = s
            .get("label")
            .and_then(|l| l.as_str())
            .map(normalize_label)
            .ok_or_else(|| AppError::Internal("seksjon mangler 'label'".into()))?;
        let lyrics = match s.get("lyrics") {
            Some(v) if v.is_array() => v
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|l| l.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            Some(v) if v.is_string() => v.as_str().unwrap_or_default().to_string(),
            _ => String::new(),
        };
        sections.push(FormattedSection { label, lyrics });
    }
    if sections.is_empty() {
        return Err(AppError::Internal(
            "formatert sang har ingen seksjoner".into(),
        ));
    }

    let mut warnings: Vec<String> = input
        .get("warnings")
        .and_then(|w| w.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let labels: std::collections::HashSet<&str> =
        sections.iter().map(|s| s.label.as_str()).collect();
    let mut arrangement: Vec<String> = Vec::new();
    if let Some(arr) = input.get("arrangement").and_then(|a| a.as_array()) {
        for entry in arr {
            if let Some(raw) = entry.as_str() {
                let norm = normalize_label(raw);
                if labels.contains(norm.as_str()) {
                    arrangement.push(norm);
                } else {
                    warnings.push(format!(
                        "Arrangement viste til ukjent del «{raw}» — hoppet over"
                    ));
                }
            }
        }
    }
    // No usable arrangement → play every section once, in order.
    if arrangement.is_empty() {
        arrangement = sections.iter().map(|s| s.label.clone()).collect();
    }

    let language = input
        .get("language")
        .and_then(|l| l.as_str())
        .unwrap_or("no")
        .to_string();
    let title_suggestion = input
        .get("title_suggestion")
        .and_then(|t| t.as_str())
        .map(String::from);

    Ok(FormattedSong {
        title_suggestion,
        language,
        sections,
        arrangement,
        warnings,
    })
}

// ── Heuristic path (pure offline fallback) ─────────────────────────────────────

/// Format raw lyrics locally, no network. Splits on blank lines, strips chord
/// lines and repetition markers, labels sections (respecting explicit headers,
/// auto-numbering verses, calling a repeated block the chorus), and proposes an
/// arrangement that walks the blocks in order with repeats collapsed to one
/// section reference.
pub fn heuristic_format(raw: &str) -> FormattedSong {
    let blocks = split_blocks(raw);

    // Each block → (header_label_or_none, cleaned_lyrics).
    struct Block {
        header: Option<String>,
        lyrics: String,
    }
    let mut parsed: Vec<Block> = Vec::new();
    for block in &blocks {
        let mut lines: Vec<&str> = block.iter().map(|s| s.as_str()).collect();
        let mut header: Option<String> = None;
        if let Some(first) = lines.first() {
            if let Some(label) = detect_header(first) {
                header = Some(label);
                lines.remove(0);
            }
        }
        let kept: Vec<String> = lines
            .into_iter()
            .filter(|l| !is_chord_line(l) && !is_repeat_marker(l))
            .map(|l| l.trim_end().to_string())
            .collect();
        let lyrics = kept.join("\n");
        if lyrics.trim().is_empty() && header.is_none() {
            continue;
        }
        parsed.push(Block { header, lyrics });
    }

    // Deduplicate identical lyric blocks into one section (the chorus pattern),
    // and assign labels.
    let mut sections: Vec<FormattedSection> = Vec::new();
    let mut arrangement: Vec<String> = Vec::new();
    let mut by_lyrics: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut verse_n = 0;

    // Pre-count repeats to decide which unlabeled block is the chorus.
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for b in &parsed {
        *counts.entry(b.lyrics.as_str()).or_default() += 1;
    }

    for b in &parsed {
        if let Some(existing) = by_lyrics.get(&b.lyrics) {
            arrangement.push(existing.clone());
            continue;
        }
        let label = match &b.header {
            Some(h) => h.clone(),
            None => {
                if counts.get(b.lyrics.as_str()).copied().unwrap_or(0) > 1 {
                    "chorus".to_string()
                } else {
                    verse_n += 1;
                    format!("verse_{verse_n}")
                }
            }
        };
        let label = unique_label(&sections, label);
        by_lyrics.insert(b.lyrics.clone(), label.clone());
        sections.push(FormattedSection {
            label: label.clone(),
            lyrics: b.lyrics.clone(),
        });
        arrangement.push(label);
    }

    let mut warnings = Vec::new();
    if sections.is_empty() {
        warnings.push("Fant ingen tekst å formatere.".to_string());
    }

    FormattedSong {
        title_suggestion: None,
        language: detect_language(raw),
        sections,
        arrangement,
        warnings,
    }
}

/// Ensure a label is unique among already-built sections (suffix `_2`, `_3`, …).
pub(crate) fn unique_label(existing: &[FormattedSection], label: String) -> String {
    if !existing.iter().any(|s| s.label == label) {
        return label;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{label}_{n}");
        if !existing.iter().any(|s| s.label == candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn split_blocks(raw: &str) -> Vec<Vec<String>> {
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
}

/// Canonicalize a section label to snake_case with synonym mapping.
pub fn normalize_label(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let cleaned: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let parts: Vec<String> = cleaned.split_whitespace().map(String::from).collect();
    let number: Option<String> = parts
        .iter()
        .find(|p| p.chars().all(|c| c.is_ascii_digit()))
        .cloned();
    let words: Vec<&str> = parts
        .iter()
        .filter(|p| !p.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.as_str())
        .collect();

    let has = |w: &str| words.contains(&w);
    let base = if words.iter().any(|w| w.starts_with("prechorus")) || (has("pre") && has("chorus"))
    {
        "pre_chorus"
    } else if words.iter().any(|w| matches!(*w, "verse" | "vers" | "v")) {
        "verse"
    } else if words
        .iter()
        .any(|w| matches!(*w, "chorus" | "refrain" | "refreng" | "kor"))
    {
        "chorus"
    } else if words.iter().any(|w| matches!(*w, "bridge" | "bro")) {
        "bridge"
    } else if has("intro") {
        "intro"
    } else if words
        .iter()
        .any(|w| matches!(*w, "outro" | "ending" | "slutt" | "utgang"))
    {
        "ending"
    } else if has("tag") {
        "tag"
    } else if words
        .iter()
        .any(|w| matches!(*w, "instrumental" | "mellomspill" | "solo"))
    {
        "instrumental"
    } else if words.is_empty() {
        "verse"
    } else {
        // Unknown label: keep the cleaned words.
        return match number {
            Some(n) => format!("{}_{}", words.join("_"), n),
            None => words.join("_"),
        };
    };

    match (base, number) {
        ("verse", Some(n)) => format!("verse_{n}"),
        ("pre_chorus", Some(n)) => format!("pre_chorus_{n}"),
        (b, _) => b.to_string(),
    }
}

/// Does this line look like a section header ("[Chorus]", "Verse 1", "Refreng:")?
pub(crate) fn detect_header(line: &str) -> Option<String> {
    let t = line.trim();
    // Headers are short and contain a known section keyword.
    let inner: String = t
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let words: Vec<&str> = inner.split_whitespace().collect();
    if words.is_empty() || words.len() > 3 {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "verse",
        "vers",
        "chorus",
        "refrain",
        "refreng",
        "prechorus",
        "pre",
        "bridge",
        "bro",
        "intro",
        "outro",
        "ending",
        "tag",
        "instrumental",
        "mellomspill",
    ];
    let is_label_word = |w: &&str| {
        KEYWORDS.contains(w) || w.starts_with("prechorus") || w.chars().all(|c| c.is_ascii_digit())
    };
    // A header is *only* label words (+ an optional number) — not a lyric line
    // that merely happens to contain a keyword ("Verse one line", "Bridge over
    // troubled water").
    let only_label_words = words.iter().all(is_label_word);
    let has_keyword = words
        .iter()
        .any(|w| KEYWORDS.contains(w) || w.starts_with("prechorus"));
    if only_label_words && has_keyword {
        Some(normalize_label(t))
    } else {
        None
    }
}

fn is_repeat_marker(line: &str) -> bool {
    let t = line.trim().to_lowercase();
    let stripped: String = t
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | '[' | ']' | '.'))
        .collect();
    let s = stripped.trim();
    if matches!(
        s,
        "x2" | "x3" | "x4" | "2x" | "3x" | "4x" | "repeat" | "gjenta"
    ) {
        return true;
    }
    // "repeat chorus" / "gjenta refreng" etc. are markers, but only when what
    // follows is a section reference — never an arbitrary lyric line that merely
    // starts with the word "Repeat"/"Gjenta".
    if let Some(rest) = s
        .strip_prefix("repeat ")
        .or_else(|| s.strip_prefix("gjenta "))
    {
        // A marker references a known section (optionally with a number) and is
        // short. Anything else is real lyrics.
        const SECTION_WORDS: &[&str] = &[
            "chorus", "refrain", "refreng", "kor", "verse", "vers", "bridge", "bro", "intro",
            "outro", "tag", "x2", "x3", "x4", "2x", "3x", "4x",
        ];
        let words: Vec<&str> = rest.split_whitespace().collect();
        if words.len() <= 2
            && words
                .iter()
                .all(|w| SECTION_WORDS.contains(w) || w.chars().all(|c| c.is_ascii_digit()))
        {
            return true;
        }
    }
    false
}

/// A chord token like `G`, `Am`, `F#m7`, `C/E`.
fn is_chord_token(tok: &str) -> bool {
    let mut chars = tok.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !('A'..='G').contains(&first.to_ascii_uppercase()) {
        return false;
    }
    // Allowed after the root: note letters A–G (slash-bass like `A/E`),
    // accidentals, extensions, and quality letters — but NOT vowels like
    // e/o/y that appear in ordinary words ("Came", "Amen", "Holy").
    const ALLOWED: &str = "ABCDEFG#b0123456789mMajisundg/+()";
    tok.chars().skip(1).all(|c| ALLOWED.contains(c))
}

/// A chord-only line: several chord tokens, or a single chord with an
/// unambiguous marker (digit/#/slash). Bare single letters are treated as
/// lyrics, not chords.
fn is_chord_line(line: &str) -> bool {
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.is_empty() {
        return false;
    }
    if !toks.iter().all(|t| is_chord_token(t)) {
        return false;
    }
    toks.len() >= 2
        || toks[0]
            .chars()
            .any(|c| c.is_ascii_digit() || matches!(c, '#' | '/'))
}

pub(crate) fn detect_language(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains('æ') || lower.contains('ø') || lower.contains('å') {
        return "no".to_string();
    }
    let no_words = [
        "og", "jeg", "det", "er", "ikke", "deg", "meg", "til", "har", "som", "din",
    ];
    let en_words = [
        "the", "and", "you", "your", "is", "of", "to", "we", "i", "in", "his",
    ];
    let tokens: Vec<String> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    let count = |set: &[&str]| tokens.iter().filter(|t| set.contains(&t.as_str())).count();
    if count(&en_words) > count(&no_words) {
        "en".to_string()
    } else {
        "no".to_string()
    }
}

// ── Apply to the database ──────────────────────────────────────────────────────

/// Materialize a [`FormattedSong`] onto an existing song: create its sections,
/// then a new "AI-forslag" arrangement wired to match `formatted.arrangement`.
/// Returns the created arrangement so the UI can activate it.
pub async fn apply_formatted_song(
    pool: &SqlitePool,
    song_id: &str,
    formatted: &FormattedSong,
) -> AppResult<SongArrangement> {
    let song_repo = SongRepo::new(pool);
    let arr_repo = ArrangementRepo::new(pool);

    // Create sections, remembering label → section id (first wins on dup label).
    let mut label_to_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut created: Vec<SongSection> = Vec::new();
    for section in &formatted.sections {
        if label_to_id.contains_key(&section.label) {
            continue;
        }
        let s = song_repo
            .add_section(song_id, &section.label, &section.lyrics)
            .await?;
        label_to_id.insert(section.label.clone(), s.id.clone());
        created.push(s);
    }
    let _ = created;

    let arrangement = arr_repo.create(song_id, "AI-forslag").await?;
    let section_ids: Vec<String> = formatted
        .arrangement
        .iter()
        .filter_map(|label| label_to_id.get(label).cloned())
        .collect();
    if !section_ids.is_empty() {
        arr_repo.set_items(&arrangement.id, &section_ids).await?;
    }
    Ok(arrangement)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize_label ──────────────────────────────────────────────────────
    #[test]
    fn normalize_label_maps_synonyms_and_numbers() {
        assert_eq!(normalize_label("Verse 1"), "verse_1");
        assert_eq!(normalize_label("[Chorus]"), "chorus");
        assert_eq!(normalize_label("Refreng"), "chorus");
        assert_eq!(normalize_label("Pre-Chorus"), "pre_chorus");
        assert_eq!(normalize_label("Bridge"), "bridge");
        assert_eq!(normalize_label("Mellomspill"), "instrumental");
    }

    // ── chord / marker detection ───────────────────────────────────────────────
    #[test]
    fn chord_lines_detected_lyrics_preserved() {
        assert!(is_chord_line("G   D   Em   C"));
        assert!(is_chord_line("F#m7  A/E"));
        assert!(!is_chord_line("Amazing grace how sweet the sound"));
        assert!(!is_chord_line("Came to save us all")); // starts with C but is lyrics
        assert!(!is_chord_line("Amen")); // single word, not a chord
    }

    #[test]
    fn repeat_markers_detected() {
        assert!(is_repeat_marker("x2"));
        assert!(is_repeat_marker("(x2)"));
        assert!(is_repeat_marker("(repeat chorus)"));
        assert!(is_repeat_marker("Gjenta refreng"));
        assert!(!is_repeat_marker("excellent"));
    }

    // Regression: a genuine lyric line whose first word happens to be
    // "Repeat"/"Gjenta" must NOT be treated as a repetition marker.
    #[test]
    fn repeat_marker_does_not_eat_real_lyrics() {
        assert!(!is_repeat_marker("Repeat after me"));
        assert!(!is_repeat_marker("Gjenta etter meg"));
        let f = heuristic_format("Repeat these words O Lord\nwith all my heart");
        assert_eq!(
            f.sections[0].lyrics,
            "Repeat these words O Lord\nwith all my heart"
        );
        let g = heuristic_format("Gjenta etter meg\nnoe annet");
        assert_eq!(g.sections[0].lyrics, "Gjenta etter meg\nnoe annet");
    }

    // ── heuristic_format ───────────────────────────────────────────────────────
    #[test]
    fn heuristic_splits_blocks_and_numbers_verses() {
        let raw = "Line a1\nLine a2\n\nLine b1\nLine b2";
        let f = heuristic_format(raw);
        assert_eq!(f.sections.len(), 2);
        assert_eq!(f.sections[0].label, "verse_1");
        assert_eq!(f.sections[1].label, "verse_2");
        assert_eq!(f.arrangement, vec!["verse_1", "verse_2"]);
    }

    #[test]
    fn heuristic_collapses_repeated_block_into_chorus() {
        let raw = "Verse one line\n\nHallelujah sing\n\nAnother verse\n\nHallelujah sing";
        let f = heuristic_format(raw);
        // Two distinct verses + one chorus (the repeated block).
        let labels: Vec<&str> = f.sections.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"chorus"), "labels: {labels:?}");
        // chorus referenced twice in the arrangement
        assert_eq!(f.arrangement.iter().filter(|l| *l == "chorus").count(), 2);
        // 3 sections total (verse_1, chorus, verse_2)
        assert_eq!(f.sections.len(), 3);
    }

    #[test]
    fn heuristic_respects_explicit_headers_and_strips_chords() {
        let raw = "[Verse 1]\nG        D\nAmazing grace\n\n[Chorus]\nHow sweet the sound";
        let f = heuristic_format(raw);
        assert_eq!(f.sections[0].label, "verse_1");
        assert_eq!(f.sections[0].lyrics, "Amazing grace"); // chord line stripped
        assert_eq!(f.sections[1].label, "chorus");
    }

    #[test]
    fn heuristic_strips_repeat_markers() {
        let raw = "Sing it loud\nx2";
        let f = heuristic_format(raw);
        assert_eq!(f.sections[0].lyrics, "Sing it loud");
    }

    #[test]
    fn heuristic_detects_language() {
        assert_eq!(heuristic_format("Du er hellig, du er fri").language, "no");
        assert_eq!(
            heuristic_format("You are holy, you are free and good").language,
            "en"
        );
    }

    // ── parse_format_response ──────────────────────────────────────────────────
    #[test]
    fn parse_accepts_array_lyrics_and_normalizes_labels() {
        let input = serde_json::json!({
            "language": "en",
            "title_suggestion": "Amazing Grace",
            "sections": [
                { "label": "Verse 1", "lyrics": ["Amazing grace", "how sweet the sound"] },
                { "label": "Chorus", "lyrics": ["My chains are gone"] }
            ],
            "arrangement": ["Verse 1", "Chorus", "Verse 1"]
        });
        let f = parse_format_response(&input).unwrap();
        assert_eq!(f.language, "en");
        assert_eq!(f.title_suggestion.as_deref(), Some("Amazing Grace"));
        assert_eq!(f.sections[0].label, "verse_1");
        assert_eq!(f.sections[0].lyrics, "Amazing grace\nhow sweet the sound");
        assert_eq!(f.arrangement, vec!["verse_1", "chorus", "verse_1"]);
    }

    #[test]
    fn parse_drops_dangling_arrangement_refs_with_warning() {
        let input = serde_json::json!({
            "language": "no",
            "sections": [{ "label": "verse_1", "lyrics": "a" }],
            "arrangement": ["verse_1", "bridge"]
        });
        let f = parse_format_response(&input).unwrap();
        assert_eq!(f.arrangement, vec!["verse_1"]);
        assert!(f.warnings.iter().any(|w| w.contains("bridge")));
    }

    #[test]
    fn parse_empty_arrangement_defaults_to_sections_in_order() {
        let input = serde_json::json!({
            "language": "no",
            "sections": [{ "label": "verse_1", "lyrics": "a" }, { "label": "chorus", "lyrics": "b" }],
            "arrangement": []
        });
        let f = parse_format_response(&input).unwrap();
        assert_eq!(f.arrangement, vec!["verse_1", "chorus"]);
    }

    #[test]
    fn parse_errors_without_sections() {
        let input = serde_json::json!({ "language": "no", "arrangement": [] });
        assert_eq!(
            parse_format_response(&input).unwrap_err().code(),
            "internal"
        );
    }

    // ── apply_formatted_song ───────────────────────────────────────────────────
    #[tokio::test]
    async fn apply_creates_sections_and_arrangement_with_repeats() {
        use crate::db::models::{LibraryInput, SongInput};
        use crate::db::repositories::{LibraryRepo, SongRepo};
        use crate::db::Database;

        let db = Database::open_in_memory().await.unwrap();
        let lib = LibraryRepo::new(&db.pool)
            .create(LibraryInput {
                name: "T".into(),
                default_locale: None,
            })
            .await
            .unwrap();
        let song = SongRepo::new(&db.pool)
            .create(SongInput {
                library_id: lib.id,
                title: "T".into(),
                language: None,
                default_key: None,
                tempo_bpm: None,
                ccli_song_id: None,
                tono_work_id: None,
                copyright_notice: None,
            })
            .await
            .unwrap();

        let formatted = FormattedSong {
            title_suggestion: None,
            language: "en".into(),
            sections: vec![
                FormattedSection {
                    label: "verse_1".into(),
                    lyrics: "v1".into(),
                },
                FormattedSection {
                    label: "chorus".into(),
                    lyrics: "c".into(),
                },
            ],
            arrangement: vec!["verse_1".into(), "chorus".into(), "chorus".into()],
            warnings: vec![],
        };
        let arr = apply_formatted_song(&db.pool, &song.id, &formatted)
            .await
            .unwrap();

        let sections = SongRepo::new(&db.pool).sections(&song.id).await.unwrap();
        assert_eq!(sections.len(), 2);
        let resolved = ArrangementRepo::new(&db.pool)
            .resolved_sections(&arr.id)
            .await
            .unwrap();
        // verse_1, chorus, chorus
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].label, "verse_1");
        assert_eq!(resolved[2].label, "chorus");
    }
}
