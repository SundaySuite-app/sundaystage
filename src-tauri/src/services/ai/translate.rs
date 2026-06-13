//! Phase 11.2 — lyric translation (engine).
//!
//! For multilingual congregations: translate a song's lines to another
//! language while preserving meaning and line structure (so a translation can
//! sit alongside the original, line for line). Uses the shared Claude client
//! (`services::ai`) via a forced tool call; the pure prompt/schema/parse +
//! validation are unit-tested, the network sits behind the `ai` feature.
//!
//! Deferred (the genuinely complex part the plan flags): persisting translations
//! as a parallel track on the Song model and rendering them as a live overlay on
//! the output. This engine returns translated lines the caller can use however
//! it likes.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::services::bible::bundled_translations;

pub const TRANSLATE_TOOL_NAME: &str = "emit_translation";

/// The 20 target languages the plan calls for (ISO-639-1).
pub const SUPPORTED_TARGETS: &[&str] = &[
    "en", "no", "sv", "da", "de", "fr", "pl", "es", "pt", "nl", "fi", "it", "is", "et", "lv", "lt",
    "ru", "uk", "sw", "zh",
];

/// Result of translating a block of lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TranslationResult.ts")]
pub struct TranslationResult {
    pub target_language: String,
    /// One translated line per source line, in order.
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn is_supported_target(lang: &str) -> bool {
    SUPPORTED_TARGETS.contains(&lang)
}

pub fn system_prompt(target: &str) -> String {
    format!(
        "You translate worship-song lyrics into the target language ({target}). \
Rules:\n\
- Preserve the meaning faithfully; this is for congregational singing/reading, \
not a literal gloss.\n\
- Return EXACTLY one translated line per source line, in the same order.\n\
- Do not merge or split lines. Do not add commentary.\n\
- Keep proper names (Jesus, God names) natural in the target language.\n\
- Call the {TRANSLATE_TOOL_NAME} tool with the result."
    )
}

pub fn tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "lines": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["lines"]
    })
}

/// The user content: the source lines, one per line, numbered for alignment.
pub fn user_content(lines: &[String]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}. {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse + validate the tool output against the source. A line-count mismatch
/// is corrected (pad/truncate) with a warning rather than failing — the show
/// must go on. Over-long translations (>1.5× the source) get a soft warning.
pub fn parse_translation(
    input: &serde_json::Value,
    source: &[String],
    target: &str,
) -> AppResult<TranslationResult> {
    let mut lines: Vec<String> = input
        .get("lines")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Internal("oversettelse mangler 'lines'".into()))?
        .iter()
        .map(|l| l.as_str().unwrap_or_default().to_string())
        .collect();

    let mut warnings = Vec::new();
    if lines.len() != source.len() {
        warnings.push(format!(
            "Oversettelsen hadde {} linjer mot {} i originalen — justert.",
            lines.len(),
            source.len()
        ));
        lines.resize(source.len(), String::new());
    }
    let src_len: usize = source.iter().map(|l| l.chars().count()).sum();
    let out_len: usize = lines.iter().map(|l| l.chars().count()).sum();
    if src_len > 0 && out_len as f64 > src_len as f64 * 1.5 {
        warnings.push(
            "Oversettelsen er vesentlig lengre enn originalen — kan trenge mindre skrift.".into(),
        );
    }

    Ok(TranslationResult {
        target_language: target.to_string(),
        lines,
        warnings,
    })
}

/// Normalize a verse line for matching: trim, collapse internal whitespace,
/// lowercase. Bundled verses are stored as single strings; a slide may break a
/// verse across lines, so we match on the trimmed line as-is. Pure.
fn norm_verse(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Try to translate ONE line offline using the bundled public-domain Bible
/// texts. If `source_line` exactly matches a bundled verse (in any bundled
/// translation), and a bundled translation in `target` carries the same
/// (book, chapter, verse), return that verse's text. Returns `None` for
/// anything not in the bundled set (lyrics, un-bundled verses). Pure — no DB,
/// no network — so it can run at compile time with no key.
///
/// This is the keyless fallback the plan calls for: even with no API key, the
/// most-projected verses translate from what ships in the binary.
pub fn bundled_verse_translation(source_line: &str, target: &str) -> Option<String> {
    let needle = norm_verse(source_line);
    if needle.is_empty() {
        return None;
    }
    // Find which (book, chapter, verse) this source line is, in any bundled set.
    let mut coord: Option<(&str, i64, i64)> = None;
    'outer: for t in bundled_translations() {
        for v in t.verses {
            if norm_verse(v.text) == needle {
                coord = Some((v.book, v.chapter, v.verse));
                break 'outer;
            }
        }
    }
    let (book, chapter, verse) = coord?;
    // Find the same verse in a bundled translation whose language is `target`.
    for t in bundled_translations() {
        if t.language != target {
            continue;
        }
        for v in t.verses {
            if v.book == book && v.chapter == chapter && v.verse == verse {
                return Some(v.text.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_targets_include_the_suite_langs() {
        for l in ["no", "en", "sv", "da", "de", "fr", "pl"] {
            assert!(is_supported_target(l), "{l} should be supported");
        }
        assert!(!is_supported_target("xx"));
    }

    #[test]
    fn user_content_numbers_lines() {
        let c = user_content(&["Holy".into(), "Holy".into()]);
        assert_eq!(c, "1. Holy\n2. Holy");
    }

    #[test]
    fn parse_pads_short_output_with_warning() {
        let src = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let input = serde_json::json!({ "lines": ["x", "y"] });
        let r = parse_translation(&input, &src, "en").unwrap();
        assert_eq!(r.lines.len(), 3);
        assert_eq!(r.lines[2], "");
        assert!(r.warnings.iter().any(|w| w.contains("linjer")));
    }

    #[test]
    fn parse_flags_overlong_translation() {
        let src = vec!["kort".to_string()];
        let input = serde_json::json!({
            "lines": ["en veldig mye lengre oversettelse enn originalen var"]
        });
        let r = parse_translation(&input, &src, "no").unwrap();
        assert_eq!(r.lines.len(), 1);
        assert!(r.warnings.iter().any(|w| w.contains("lengre")));
    }

    #[test]
    fn parse_errors_without_lines_field() {
        let input = serde_json::json!({ "nope": true });
        assert!(parse_translation(&input, &[], "en").is_err());
    }

    #[test]
    fn bundled_verse_translates_john_316_en_to_no() {
        // KJV John 3:16 → NB1930 John 3:16, both bundled, no key needed.
        let src = "For God so loved the world, that he gave his only begotten Son, \
                   that whosoever believeth in him should not perish, but have everlasting life.";
        let out = bundled_verse_translation(src, "no").expect("bundled NO verse");
        assert!(out.contains("Gud elsket verden"), "got: {out}");
    }

    #[test]
    fn bundled_verse_matches_despite_whitespace_differences() {
        // Slide text may have collapsed/extra whitespace; matching is tolerant.
        let src = "  The   LORD is my shepherd;  I shall not want.  ";
        let out = bundled_verse_translation(src, "no").expect("Ps 23:1 in NO");
        assert!(out.contains("Herren er min hyrde"), "got: {out}");
    }

    #[test]
    fn bundled_verse_returns_none_for_unbundled_or_lyrics() {
        // A lyric line is not in the bundled Bible set.
        assert!(bundled_verse_translation("Amazing grace how sweet the sound", "no").is_none());
        // Blank line → None.
        assert!(bundled_verse_translation("   ", "no").is_none());
        // No bundled translation exists for German, so even a known verse misses.
        let src = "The LORD is my shepherd; I shall not want.";
        assert!(bundled_verse_translation(src, "de").is_none());
    }
}
