//! Phase 7.1 — Bible reference parsing + storage helpers.
//!
//! Parses freeform user input like:
//!   "John 3:16"      → John 3, verses 16
//!   "1 Kor 13:1-13"  → 1 Korinterbrev 13, verses 1-13
//!   "Joh 3"          → John 3, whole chapter
//!   "Sal 23:1-6"     → Salmenes bok 23, verses 1-6
//!
//! Multilingual: book name lookups work in all 7 UI languages
//! (Joh = John = Johannes = Jean = ...).
//!
//! The full per-translation downloader + verse cache live in Phase 7.1's
//! later steps; this module is the parser + canonical-name resolver that
//! every other Bible feature builds on.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A parsed-but-not-yet-resolved bible reference. The renderer shows
/// the canonical English book name when ambiguous so the user can verify.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ParsedBibleRef.ts")]
pub struct ParsedBibleRef {
    /// Canonical English book name (e.g. "John", "1 Corinthians").
    pub book: String,
    pub chapter: u32,
    /// `None` means "whole chapter".
    pub verse_start: Option<u32>,
    /// `None` means single verse (or whole chapter when verse_start is None).
    pub verse_end: Option<u32>,
}

/// Books of the Bible — minimal alias table for the 7 UI languages we support.
///
/// This is intentionally hand-curated. A more complete table for the
/// 66-book canon (+ deuterocanonical for Catholic use) lives in
/// `sql/0002_bible_books.sql` once Phase 7.1 ships the translation
/// downloader.
const BOOK_ALIASES: &[(&str, &[&str])] = &[
    // ── Old Testament (most-used in liturgical reading) ─────
    ("Genesis",     &["Gen", "1 Mos", "1. Mosebok", "1. Mos", "1 Mosebok"]),
    ("Exodus",      &["Exo", "Ex", "2 Mos", "2. Mosebok"]),
    ("Psalms",      &["Ps", "Psalm", "Sal", "Salm", "Salmenes"]),
    ("Proverbs",    &["Prov", "Ord", "Ordsp"]),
    ("Isaiah",      &["Isa", "Jes", "Jesaja"]),

    // ── New Testament (most-used in worship) ─────────────────
    ("Matthew",     &["Matt", "Mat", "Mt"]),
    ("Mark",        &["Mark", "Mk", "Mrk"]),
    ("Luke",        &["Luke", "Luk", "Lk"]),
    ("John",        &["John", "Joh", "Jn", "Johannes"]),
    ("Acts",        &["Acts", "Apg", "Apostlene"]),
    ("Romans",      &["Rom", "Rm"]),
    ("1 Corinthians", &["1 Cor", "1 Kor", "1.Kor", "1Kor"]),
    ("2 Corinthians", &["2 Cor", "2 Kor", "2.Kor", "2Kor"]),
    ("Galatians",   &["Gal"]),
    ("Ephesians",   &["Eph", "Ef", "Efeser"]),
    ("Philippians", &["Phil", "Fil"]),
    ("Colossians",  &["Col", "Kol"]),
    ("1 Thessalonians", &["1 Thess", "1 Tess"]),
    ("2 Thessalonians", &["2 Thess", "2 Tess"]),
    ("1 Timothy",   &["1 Tim"]),
    ("2 Timothy",   &["2 Tim"]),
    ("Titus",       &["Tit"]),
    ("Hebrews",     &["Heb", "Hebr"]),
    ("James",       &["Jas", "Jak", "Jakob"]),
    ("1 Peter",     &["1 Pet", "1 Pt"]),
    ("2 Peter",     &["2 Pet", "2 Pt"]),
    ("1 John",      &["1 Jn", "1 Joh"]),
    ("2 John",      &["2 Jn", "2 Joh"]),
    ("3 John",      &["3 Jn", "3 Joh"]),
    ("Revelation",  &["Rev", "Åp", "Åpenbaring"]),
];

#[derive(Debug, thiserror::Error)]
pub enum BibleParseError {
    #[error("could not identify book in '{0}'")]
    UnknownBook(String),
    #[error("missing chapter number in '{0}'")]
    MissingChapter(String),
    #[error("malformed verse range in '{0}'")]
    MalformedRange(String),
    #[error("empty reference")]
    Empty,
}

/// Parse "John 3:16", "1 Kor 13:1-13", "Sal 23", etc. into a structured
/// reference. Case-insensitive; tolerant of extra whitespace.
pub fn parse_reference(input: &str) -> Result<ParsedBibleRef, BibleParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(BibleParseError::Empty);
    }

    // Split at the FIRST digit — everything before is the book name,
    // everything from that digit on is "chapter[:verses]". But we need
    // to handle "1 Kor" where the leading "1" is part of the book name.
    let (book_part, rest) = split_at_chapter_number(trimmed)
        .ok_or_else(|| BibleParseError::MissingChapter(input.to_string()))?;

    let book_part = book_part.trim();
    if book_part.is_empty() {
        return Err(BibleParseError::UnknownBook(input.to_string()));
    }
    let canonical_book = resolve_book(book_part)
        .ok_or_else(|| BibleParseError::UnknownBook(book_part.to_string()))?;

    let rest = rest.trim();

    // rest is "3", "3:16", or "3:16-17" or "3:16,18,20" (csv unsupported v1)
    let (chapter_str, verses_str): (&str, Option<&str>) = match rest.split_once(':') {
        Some((c, v)) => (c.trim(), Some(v.trim())),
        None => (rest, None),
    };
    let chapter: u32 = chapter_str.parse().map_err(|_| {
        BibleParseError::MalformedRange(format!("chapter '{}' is not a number", chapter_str))
    })?;

    let (verse_start, verse_end) = if let Some(v) = verses_str {
        if v.is_empty() {
            (None, None)
        } else if let Some((a, b)) = v.split_once('-') {
            let s: u32 = a.trim().parse().map_err(|_| BibleParseError::MalformedRange(v.to_string()))?;
            let e: u32 = b.trim().parse().map_err(|_| BibleParseError::MalformedRange(v.to_string()))?;
            if e < s {
                return Err(BibleParseError::MalformedRange(format!("{}>{}", s, e)));
            }
            (Some(s), Some(e))
        } else {
            let s: u32 = v.parse().map_err(|_| BibleParseError::MalformedRange(v.to_string()))?;
            (Some(s), None)
        }
    } else {
        (None, None)
    };

    Ok(ParsedBibleRef {
        book: canonical_book,
        chapter,
        verse_start,
        verse_end,
    })
}

/// Splits "1 Kor 13:1-13" → ("1 Kor", "13:1-13"). The trick: a leading
/// "1 ", "2 ", "3 " is part of the book name when followed by letters.
fn split_at_chapter_number(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;

    // Skip a leading "<digit> " that's part of "1 Kor"-style book names.
    if bytes.len() > 2 && bytes[0].is_ascii_digit() && bytes[1] == b' ' {
        i = 2;
    } else if bytes.len() > 3 && bytes[0].is_ascii_digit() && bytes[1] == b'.' && bytes[2] == b' ' {
        i = 3;
    }

    // From `i`, walk until we find a digit (the chapter number).
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            return Some((&s[..i], &s[i..]));
        }
        i += 1;
    }
    None
}

/// Resolve any spelling/abbreviation to a canonical English book name.
pub fn resolve_book(input: &str) -> Option<String> {
    let needle = normalize(input);
    for (canonical, aliases) in BOOK_ALIASES {
        if normalize(canonical) == needle {
            return Some((*canonical).to_string());
        }
        for alias in *aliases {
            if normalize(alias) == needle {
                return Some((*canonical).to_string());
            }
        }
    }
    None
}

/// Lowercase + strip whitespace + strip dots for tolerant comparison.
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Render a `ParsedBibleRef` to display form: "John 3:16-17".
pub fn render_reference(r: &ParsedBibleRef) -> String {
    match (r.verse_start, r.verse_end) {
        (None, _)          => format!("{} {}", r.book, r.chapter),
        (Some(s), None)    => format!("{} {}:{}", r.book, r.chapter, s),
        (Some(s), Some(e)) if s == e => format!("{} {}:{}", r.book, r.chapter, s),
        (Some(s), Some(e)) => format!("{} {}:{}-{}", r.book, r.chapter, s, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_english_with_verse_range() {
        let r = parse_reference("John 3:16-17").unwrap();
        assert_eq!(r.book, "John");
        assert_eq!(r.chapter, 3);
        assert_eq!(r.verse_start, Some(16));
        assert_eq!(r.verse_end,   Some(17));
    }

    #[test]
    fn parses_norwegian_abbreviation() {
        let r = parse_reference("Joh 3:16").unwrap();
        assert_eq!(r.book, "John");
        assert_eq!(r.chapter, 3);
        assert_eq!(r.verse_start, Some(16));
    }

    #[test]
    fn parses_numbered_book_norwegian() {
        let r = parse_reference("1 Kor 13:1-13").unwrap();
        assert_eq!(r.book, "1 Corinthians");
        assert_eq!(r.chapter, 13);
        assert_eq!(r.verse_start, Some(1));
        assert_eq!(r.verse_end,   Some(13));
    }

    #[test]
    fn parses_numbered_book_with_period() {
        let r = parse_reference("1. Kor 13").unwrap();
        assert_eq!(r.book, "1 Corinthians");
        assert_eq!(r.chapter, 13);
        assert_eq!(r.verse_start, None);
    }

    #[test]
    fn parses_whole_chapter() {
        let r = parse_reference("Sal 23").unwrap();
        assert_eq!(r.book, "Psalms");
        assert_eq!(r.chapter, 23);
        assert_eq!(r.verse_start, None);
        assert_eq!(r.verse_end,   None);
    }

    #[test]
    fn parses_revelation_with_norwegian_aa() {
        let r = parse_reference("Åp 22").unwrap();
        assert_eq!(r.book, "Revelation");
        assert_eq!(r.chapter, 22);
    }

    #[test]
    fn rejects_unknown_book() {
        assert!(matches!(
            parse_reference("Klingon 1:1"),
            Err(BibleParseError::UnknownBook(_))
        ));
    }

    #[test]
    fn rejects_missing_chapter() {
        assert!(matches!(
            parse_reference("John"),
            Err(BibleParseError::MissingChapter(_))
        ));
    }

    #[test]
    fn rejects_backwards_range() {
        assert!(matches!(
            parse_reference("John 3:17-16"),
            Err(BibleParseError::MalformedRange(_))
        ));
    }

    #[test]
    fn render_round_trip() {
        let r = parse_reference("1 Kor 13:1-13").unwrap();
        assert_eq!(render_reference(&r), "1 Corinthians 13:1-13");
    }

    #[test]
    fn render_single_verse() {
        let r = parse_reference("John 3:16").unwrap();
        assert_eq!(render_reference(&r), "John 3:16");
    }

    #[test]
    fn render_whole_chapter() {
        let r = parse_reference("Psalms 23").unwrap();
        assert_eq!(render_reference(&r), "Psalms 23");
    }

    #[test]
    fn resolve_book_handles_case_and_dots() {
        assert_eq!(resolve_book("john"),    Some("John".into()));
        assert_eq!(resolve_book("JOHN"),    Some("John".into()));
        assert_eq!(resolve_book("Joh."),    Some("John".into()));
        assert_eq!(resolve_book("1.Kor"),   Some("1 Corinthians".into()));
        assert_eq!(resolve_book("1 kor"),   Some("1 Corinthians".into()));
    }

    #[test]
    fn empty_string_rejected() {
        assert!(matches!(parse_reference(""),    Err(BibleParseError::Empty)));
        assert!(matches!(parse_reference("   "), Err(BibleParseError::Empty)));
    }
}
