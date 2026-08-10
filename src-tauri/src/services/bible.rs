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

/// One book of the 66-book Protestant canon: the single source of truth for its
/// canonical name, ordering, localized display and every spelling we accept —
/// including the exact `books[].name` the scrollmapper corpora use (their
/// numbered books are roman-numeralled — "I Corinthians" — and Revelation is
/// "Revelation of John"), which the corpus downloader maps back to
/// `canonical`/`order`.
///
/// This is the table the old comment promised would live in a
/// `sql/0002_bible_books.sql` migration. It never needed to be relational: the
/// `bible_verse` rows already carry `book`/`book_order`, so a book-metadata
/// table would be dead weight that could only drift out of sync with the
/// resolver. The canon lives here, in Rust, as the one place that turns a
/// spelling into a canonical book — the same shape the parser always used, now
/// complete.
pub struct BookCanon {
    /// Canonical English name stored in `bible_verse.book` (e.g. "Revelation").
    pub canonical: &'static str,
    /// Canonical 1..=66 ordering stored in `bible_verse.book_order`.
    pub order: i64,
    /// Norwegian display name (Bibelen 1930 spelling).
    pub no: &'static str,
    /// Exact `books[].name` spellings the scrollmapper corpora use, when they
    /// differ from `canonical` (roman-numeral numbered books, "Revelation of
    /// John"). Empty when the source spelling already equals `canonical`.
    pub source_names: &'static [&'static str],
    /// Abbreviations and multilingual spellings accepted by the reference parser
    /// (English + Norwegian at minimum; the seven-language set is kept for the
    /// books that already had it).
    pub aliases: &'static [&'static str],
}

/// The full 66-book canon. Order is the canonical Protestant ordering (1..=66).
/// Kept as a one-line-per-book table (rustfmt would explode each literal across
/// seven lines and destroy the alignment that makes it reviewable).
#[rustfmt::skip]
const CANON: &[BookCanon] = &[
    // ── Old Testament ────────────────────────────────────────────────────────
    BookCanon { canonical: "Genesis", order: 1, no: "1. Mosebok", source_names: &[], aliases: &["Gen", "1 Mos", "1. Mosebok", "1. Mos", "1 Mosebok"] },
    BookCanon { canonical: "Exodus", order: 2, no: "2. Mosebok", source_names: &[], aliases: &["Exo", "Ex", "2 Mos", "2. Mosebok"] },
    BookCanon { canonical: "Leviticus", order: 3, no: "3. Mosebok", source_names: &[], aliases: &["Lev", "3 Mos", "3. Mosebok"] },
    BookCanon { canonical: "Numbers", order: 4, no: "4. Mosebok", source_names: &[], aliases: &["Num", "4 Mos", "4. Mosebok"] },
    BookCanon { canonical: "Deuteronomy", order: 5, no: "5. Mosebok", source_names: &[], aliases: &["Deut", "Dt", "5 Mos", "5. Mosebok"] },
    BookCanon { canonical: "Joshua", order: 6, no: "Josva", source_names: &[], aliases: &["Josh", "Jos", "Josva"] },
    BookCanon { canonical: "Judges", order: 7, no: "Dommerne", source_names: &[], aliases: &["Judg", "Dom", "Dommerne"] },
    BookCanon { canonical: "Ruth", order: 8, no: "Rut", source_names: &[], aliases: &["Rut"] },
    BookCanon { canonical: "1 Samuel", order: 9, no: "1. Samuelsbok", source_names: &["I Samuel"], aliases: &["1 Sam", "1 Sm", "1. Samuel", "1 Samuelsbok"] },
    BookCanon { canonical: "2 Samuel", order: 10, no: "2. Samuelsbok", source_names: &["II Samuel"], aliases: &["2 Sam", "2 Sm", "2. Samuel", "2 Samuelsbok"] },
    BookCanon { canonical: "1 Kings", order: 11, no: "1. Kongebok", source_names: &["I Kings"], aliases: &["1 Kgs", "1 Kong", "1. Kong", "1 Kongebok"] },
    BookCanon { canonical: "2 Kings", order: 12, no: "2. Kongebok", source_names: &["II Kings"], aliases: &["2 Kgs", "2 Kong", "2. Kong", "2 Kongebok"] },
    BookCanon { canonical: "1 Chronicles", order: 13, no: "1. Krønikebok", source_names: &["I Chronicles"], aliases: &["1 Chr", "1 Krøn", "1. Krøn", "1 Krønikebok"] },
    BookCanon { canonical: "2 Chronicles", order: 14, no: "2. Krønikebok", source_names: &["II Chronicles"], aliases: &["2 Chr", "2 Krøn", "2. Krøn", "2 Krønikebok"] },
    BookCanon { canonical: "Ezra", order: 15, no: "Esra", source_names: &[], aliases: &["Esra"] },
    BookCanon { canonical: "Nehemiah", order: 16, no: "Nehemja", source_names: &[], aliases: &["Neh", "Nehemja"] },
    BookCanon { canonical: "Esther", order: 17, no: "Ester", source_names: &[], aliases: &["Est", "Ester"] },
    BookCanon { canonical: "Job", order: 18, no: "Job", source_names: &[], aliases: &[] },
    BookCanon { canonical: "Psalms", order: 19, no: "Salmenes bok", source_names: &[], aliases: &["Ps", "Psalm", "Sal", "Salm", "Salmenes"] },
    BookCanon { canonical: "Proverbs", order: 20, no: "Ordspråkene", source_names: &[], aliases: &["Prov", "Ord", "Ordsp", "Ordspråkene"] },
    BookCanon { canonical: "Ecclesiastes", order: 21, no: "Forkynneren", source_names: &[], aliases: &["Eccl", "Fork", "Pred", "Forkynneren"] },
    BookCanon { canonical: "Song of Solomon", order: 22, no: "Høisangen", source_names: &[], aliases: &["Song", "Song of Songs", "Høys", "Høisangen", "Høysangen"] },
    BookCanon { canonical: "Isaiah", order: 23, no: "Jesaja", source_names: &[], aliases: &["Isa", "Jes", "Jesaja"] },
    BookCanon { canonical: "Jeremiah", order: 24, no: "Jeremia", source_names: &[], aliases: &["Jer", "Jeremia"] },
    BookCanon { canonical: "Lamentations", order: 25, no: "Klagesangene", source_names: &[], aliases: &["Lam", "Klag", "Klagesangene"] },
    BookCanon { canonical: "Ezekiel", order: 26, no: "Esekiel", source_names: &[], aliases: &["Ezek", "Esek", "Esekiel"] },
    BookCanon { canonical: "Daniel", order: 27, no: "Daniel", source_names: &[], aliases: &["Dan"] },
    BookCanon { canonical: "Hosea", order: 28, no: "Hosea", source_names: &[], aliases: &["Hos"] },
    BookCanon { canonical: "Joel", order: 29, no: "Joel", source_names: &[], aliases: &[] },
    BookCanon { canonical: "Amos", order: 30, no: "Amos", source_names: &[], aliases: &[] },
    BookCanon { canonical: "Obadiah", order: 31, no: "Obadja", source_names: &[], aliases: &["Obad", "Ob", "Obadja"] },
    BookCanon { canonical: "Jonah", order: 32, no: "Jonas", source_names: &[], aliases: &["Jona", "Jonas"] },
    BookCanon { canonical: "Micah", order: 33, no: "Mika", source_names: &[], aliases: &["Mic", "Mika"] },
    BookCanon { canonical: "Nahum", order: 34, no: "Nahum", source_names: &[], aliases: &["Nah"] },
    BookCanon { canonical: "Habakkuk", order: 35, no: "Habakkuk", source_names: &[], aliases: &["Hab"] },
    BookCanon { canonical: "Zephaniah", order: 36, no: "Sefanja", source_names: &[], aliases: &["Zeph", "Sef", "Sefanja"] },
    BookCanon { canonical: "Haggai", order: 37, no: "Haggai", source_names: &[], aliases: &["Hag"] },
    BookCanon { canonical: "Zechariah", order: 38, no: "Sakarja", source_names: &[], aliases: &["Zech", "Sak", "Sakarja"] },
    BookCanon { canonical: "Malachi", order: 39, no: "Malaki", source_names: &[], aliases: &["Mal", "Malaki"] },
    // ── New Testament ────────────────────────────────────────────────────────
    BookCanon { canonical: "Matthew", order: 40, no: "Matteus", source_names: &[], aliases: &["Matt", "Mat", "Mt", "Matteus"] },
    BookCanon { canonical: "Mark", order: 41, no: "Markus", source_names: &[], aliases: &["Mk", "Mrk", "Markus"] },
    BookCanon { canonical: "Luke", order: 42, no: "Lukas", source_names: &[], aliases: &["Luk", "Lk", "Lukas"] },
    BookCanon { canonical: "John", order: 43, no: "Johannes", source_names: &[], aliases: &["Joh", "Jn", "Johannes"] },
    BookCanon { canonical: "Acts", order: 44, no: "Apostlenes gjerninger", source_names: &[], aliases: &["Apg", "Apostlene", "Apostlenes gjerninger"] },
    BookCanon { canonical: "Romans", order: 45, no: "Romerne", source_names: &[], aliases: &["Rom", "Rm", "Romerne"] },
    BookCanon { canonical: "1 Corinthians", order: 46, no: "1. Korinterbrev", source_names: &["I Corinthians"], aliases: &["1 Cor", "1 Kor", "1.Kor", "1Kor", "1. Korinterbrev"] },
    BookCanon { canonical: "2 Corinthians", order: 47, no: "2. Korinterbrev", source_names: &["II Corinthians"], aliases: &["2 Cor", "2 Kor", "2.Kor", "2Kor", "2. Korinterbrev"] },
    BookCanon { canonical: "Galatians", order: 48, no: "Galaterne", source_names: &[], aliases: &["Gal", "Galaterne"] },
    BookCanon { canonical: "Ephesians", order: 49, no: "Efeserne", source_names: &[], aliases: &["Eph", "Ef", "Efeser", "Efeserne"] },
    BookCanon { canonical: "Philippians", order: 50, no: "Filipperne", source_names: &[], aliases: &["Phil", "Fil", "Filipperne"] },
    BookCanon { canonical: "Colossians", order: 51, no: "Kolosserne", source_names: &[], aliases: &["Col", "Kol", "Kolosserne"] },
    BookCanon { canonical: "1 Thessalonians", order: 52, no: "1. Tessalonikerbrev", source_names: &["I Thessalonians"], aliases: &["1 Thess", "1 Tess", "1. Tess"] },
    BookCanon { canonical: "2 Thessalonians", order: 53, no: "2. Tessalonikerbrev", source_names: &["II Thessalonians"], aliases: &["2 Thess", "2 Tess", "2. Tess"] },
    BookCanon { canonical: "1 Timothy", order: 54, no: "1. Timoteus", source_names: &["I Timothy"], aliases: &["1 Tim", "1. Tim"] },
    BookCanon { canonical: "2 Timothy", order: 55, no: "2. Timoteus", source_names: &["II Timothy"], aliases: &["2 Tim", "2. Tim"] },
    BookCanon { canonical: "Titus", order: 56, no: "Titus", source_names: &[], aliases: &["Tit"] },
    BookCanon { canonical: "Philemon", order: 57, no: "Filemon", source_names: &[], aliases: &["Phlm", "Filem", "Filemon"] },
    BookCanon { canonical: "Hebrews", order: 58, no: "Hebreerne", source_names: &[], aliases: &["Heb", "Hebr", "Hebreerne"] },
    BookCanon { canonical: "James", order: 59, no: "Jakob", source_names: &[], aliases: &["Jas", "Jak", "Jakob"] },
    BookCanon { canonical: "1 Peter", order: 60, no: "1. Peter", source_names: &["I Peter"], aliases: &["1 Pet", "1 Pt", "1. Pet"] },
    BookCanon { canonical: "2 Peter", order: 61, no: "2. Peter", source_names: &["II Peter"], aliases: &["2 Pet", "2 Pt", "2. Pet"] },
    BookCanon { canonical: "1 John", order: 62, no: "1. Johannes", source_names: &["I John"], aliases: &["1 Jn", "1 Joh", "1. Joh"] },
    BookCanon { canonical: "2 John", order: 63, no: "2. Johannes", source_names: &["II John"], aliases: &["2 Jn", "2 Joh", "2. Joh"] },
    BookCanon { canonical: "3 John", order: 64, no: "3. Johannes", source_names: &["III John"], aliases: &["3 Jn", "3 Joh", "3. Joh"] },
    BookCanon { canonical: "Jude", order: 65, no: "Judas", source_names: &[], aliases: &["Jud", "Judas"] },
    BookCanon { canonical: "Revelation", order: 66, no: "Åpenbaringen", source_names: &["Revelation of John"], aliases: &["Rev", "Åp", "Åpenbaring", "Åpenbaringen"] },
];

/// Find the canon entry for any accepted spelling of a book — canonical name,
/// Norwegian display name, a scrollmapper source spelling, or an alias.
fn lookup(input: &str) -> Option<&'static BookCanon> {
    let needle = normalize(input);
    CANON.iter().find(|b| {
        normalize(b.canonical) == needle
            || normalize(b.no) == needle
            || b.source_names.iter().any(|s| normalize(s) == needle)
            || b.aliases.iter().any(|a| normalize(a) == needle)
    })
}

/// Map a scrollmapper `books[].name` to our (canonical name, canonical order).
/// Used by the corpus downloader. Falls through the general resolver, so any
/// accepted spelling maps — not only the exact source spelling.
pub fn canonical_for_source_name(name: &str) -> Option<(&'static str, i64)> {
    lookup(name).map(|b| (b.canonical, b.order))
}

/// The canonical 1..=66 order for a canonical book name.
pub fn book_order(canonical: &str) -> Option<i64> {
    lookup(canonical).map(|b| b.order)
}

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
            let s: u32 = a
                .trim()
                .parse()
                .map_err(|_| BibleParseError::MalformedRange(v.to_string()))?;
            let e: u32 = b
                .trim()
                .parse()
                .map_err(|_| BibleParseError::MalformedRange(v.to_string()))?;
            if e < s {
                return Err(BibleParseError::MalformedRange(format!("{}>{}", s, e)));
            }
            (Some(s), Some(e))
        } else {
            let s: u32 = v
                .parse()
                .map_err(|_| BibleParseError::MalformedRange(v.to_string()))?;
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
    lookup(input).map(|b| b.canonical.to_string())
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
        (None, _) => format!("{} {}", r.book, r.chapter),
        (Some(s), None) => format!("{} {}:{}", r.book, r.chapter, s),
        (Some(s), Some(e)) if s == e => format!("{} {}:{}", r.book, r.chapter, s),
        (Some(s), Some(e)) => format!("{} {}:{}-{}", r.book, r.chapter, s, e),
    }
}

// ── Book display names (localized) ──────────────────────────────────────────

/// Norwegian display name for a canonical English book name. Falls back to the
/// canonical name for anything not in the canon (and for non-Norwegian locales,
/// which display the canonical English name).
pub fn book_display(canonical: &str, lang: &str) -> String {
    if lang == "no" {
        if let Some(b) = lookup(canonical) {
            return b.no.to_string();
        }
    }
    canonical.to_string()
}

// ── Bundled public-domain text (curated starter set) ─────────────────────────

pub struct SeedVerse {
    pub book: &'static str,
    pub book_order: i64,
    pub chapter: i64,
    pub verse: i64,
    pub text: &'static str,
}

pub struct SeedTranslation {
    pub code: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    pub verses: &'static [SeedVerse],
}

/// Bundled translations. A curated set of the passages churches actually
/// project — enough to browse, search, and compare out of the box. A full
/// 66-book import is the (network-bound) downloader follow-up. KJV and Bibelen
/// 1930 are both public domain.
pub fn bundled_translations() -> &'static [SeedTranslation] {
    &[
        SeedTranslation {
            code: "KJV",
            name: "King James Version",
            language: "en",
            verses: KJV,
        },
        SeedTranslation {
            code: "NB1930",
            name: "Bibelen 1930",
            language: "no",
            verses: NB1930,
        },
    ]
}

macro_rules! v {
    ($book:expr, $order:expr, $ch:expr, $vs:expr, $text:expr) => {
        SeedVerse {
            book: $book,
            book_order: $order,
            chapter: $ch,
            verse: $vs,
            text: $text,
        }
    };
}

const KJV: &[SeedVerse] = &[
    v!("John", 43, 1, 1, "In the beginning was the Word, and the Word was with God, and the Word was God."),
    v!("John", 43, 1, 2, "The same was in the beginning with God."),
    v!("John", 43, 1, 3, "All things were made by him; and without him was not any thing made that was made."),
    v!("John", 43, 1, 4, "In him was life; and the life was the light of men."),
    v!("John", 43, 1, 5, "And the light shineth in darkness; and the darkness comprehended it not."),
    v!("John", 43, 3, 16, "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."),
    v!("Psalms", 19, 23, 1, "The LORD is my shepherd; I shall not want."),
    v!("Psalms", 19, 23, 2, "He maketh me to lie down in green pastures: he leadeth me beside the still waters."),
    v!("Psalms", 19, 23, 3, "He restoreth my soul: he leadeth me in the paths of righteousness for his name's sake."),
    v!("Psalms", 19, 23, 4, "Yea, though I walk through the valley of the shadow of death, I will fear no evil: for thou art with me; thy rod and thy staff they comfort me."),
    v!("Psalms", 19, 23, 5, "Thou preparest a table before me in the presence of mine enemies: thou anointest my head with oil; my cup runneth over."),
    v!("Psalms", 19, 23, 6, "Surely goodness and mercy shall follow me all the days of my life: and I will dwell in the house of the LORD for ever."),
    v!("1 Corinthians", 46, 13, 4, "Charity suffereth long, and is kind; charity envieth not; charity vaunteth not itself, is not puffed up,"),
    v!("1 Corinthians", 46, 13, 5, "Doth not behave itself unseemly, seeketh not her own, is not easily provoked, thinketh no evil;"),
    v!("1 Corinthians", 46, 13, 6, "Rejoiceth not in iniquity, but rejoiceth in the truth;"),
    v!("1 Corinthians", 46, 13, 7, "Beareth all things, believeth all things, hopeth all things, endureth all things."),
    v!("Philippians", 50, 4, 6, "Be careful for nothing; but in every thing by prayer and supplication with thanksgiving let your requests be made known unto God."),
    v!("Philippians", 50, 4, 7, "And the peace of God, which passeth all understanding, shall keep your hearts and minds through Christ Jesus."),
    v!("Romans", 45, 8, 28, "And we know that all things work together for good to them that love God, to them who are the called according to his purpose."),
    v!("Matthew", 40, 11, 28, "Come unto me, all ye that labour and are heavy laden, and I will give you rest."),
    v!("Isaiah", 23, 41, 10, "Fear thou not; for I am with thee: be not dismayed; for I am thy God: I will strengthen thee; yea, I will help thee; yea, I will uphold thee with the right hand of my righteousness."),
];

const NB1930: &[SeedVerse] = &[
    v!("John", 43, 3, 16, "For så har Gud elsket verden at han gav sin Sønn, den enbårne, forat hver den som tror på ham, ikke skal fortapes, men ha evig liv."),
    v!("Psalms", 19, 23, 1, "Herren er min hyrde, mig fattes intet."),
    v!("Psalms", 19, 23, 2, "Han lar mig ligge i grønne enger, han leder mig til hvilens vann."),
    v!("Psalms", 19, 23, 3, "Han vederkveger min sjel, han fører mig på rettferdighets stier for sitt navns skyld."),
    v!("Psalms", 19, 23, 4, "Om jeg enn skulde vandre i dødsskyggens dal, frykter jeg ikke for ondt; for du er med mig, din kjepp og din stav de trøster mig."),
    v!("Psalms", 19, 23, 5, "Du dekker bord for mig like for mine fienders øine, du salver mitt hode med olje; mitt beger flyter over."),
    v!("Psalms", 19, 23, 6, "Bare godhet og miskunnhet skal efterjage mig alle mitt livs dager, og jeg skal bo i Herrens hus gjennem lange tider."),
    v!("Matthew", 40, 11, 28, "Kom til mig, alle I som strever og har tungt å bære, og jeg vil gi eder hvile!"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_set_is_coherent() {
        for t in bundled_translations() {
            assert!(!t.verses.is_empty(), "{} has no verses", t.code);
            for verse in t.verses {
                assert!(
                    resolve_book(verse.book).is_some(),
                    "unknown book {}",
                    verse.book
                );
                assert!(!verse.text.trim().is_empty());
            }
        }
    }

    #[test]
    fn book_display_localizes_norwegian() {
        assert_eq!(book_display("John", "no"), "Johannes");
        assert_eq!(book_display("Psalms", "no"), "Salmenes bok");
        assert_eq!(book_display("John", "en"), "John");
    }

    #[test]
    fn parses_english_with_verse_range() {
        let r = parse_reference("John 3:16-17").unwrap();
        assert_eq!(r.book, "John");
        assert_eq!(r.chapter, 3);
        assert_eq!(r.verse_start, Some(16));
        assert_eq!(r.verse_end, Some(17));
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
        assert_eq!(r.verse_end, Some(13));
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
        assert_eq!(r.verse_end, None);
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
        assert_eq!(resolve_book("john"), Some("John".into()));
        assert_eq!(resolve_book("JOHN"), Some("John".into()));
        assert_eq!(resolve_book("Joh."), Some("John".into()));
        assert_eq!(resolve_book("1.Kor"), Some("1 Corinthians".into()));
        assert_eq!(resolve_book("1 kor"), Some("1 Corinthians".into()));
    }

    #[test]
    fn empty_string_rejected() {
        assert!(matches!(parse_reference(""), Err(BibleParseError::Empty)));
        assert!(matches!(
            parse_reference("   "),
            Err(BibleParseError::Empty)
        ));
    }

    // ── Full canon (C1) ─────────────────────────────────────────────────────

    #[test]
    fn canon_is_the_whole_66_book_protestant_bible() {
        assert_eq!(CANON.len(), 66, "the canon must be complete");
        // Orders are exactly 1..=66, each used once — no gap, no duplicate.
        let mut orders: Vec<i64> = CANON.iter().map(|b| b.order).collect();
        orders.sort_unstable();
        assert_eq!(orders, (1..=66).collect::<Vec<_>>());
        // Canonical names are unique.
        let mut names: Vec<&str> = CANON.iter().map(|b| b.canonical).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 66, "canonical names must be unique");
    }

    #[test]
    fn scrollmapper_source_names_map_to_our_canon() {
        // The three shapes the downloader must survive: roman-numeral numbered
        // books, "Revelation of John", and a plain name that already matches.
        assert_eq!(
            canonical_for_source_name("Revelation of John"),
            Some(("Revelation", 66))
        );
        assert_eq!(
            canonical_for_source_name("I Corinthians"),
            Some(("1 Corinthians", 46))
        );
        assert_eq!(canonical_for_source_name("III John"), Some(("3 John", 64)));
        assert_eq!(canonical_for_source_name("Psalms"), Some(("Psalms", 19)));
        assert_eq!(canonical_for_source_name("Genesis"), Some(("Genesis", 1)));
        // A book name outside the canon is refused, not silently dropped.
        assert_eq!(canonical_for_source_name("Tobit"), None);
    }

    #[test]
    fn every_scrollmapper_source_name_resolves() {
        // Whatever a book's declared source spelling is, the resolver must find
        // it — this is the exact call `parse_corpus` makes for each book.
        for b in CANON {
            for s in b.source_names {
                assert_eq!(
                    canonical_for_source_name(s),
                    Some((b.canonical, b.order)),
                    "source name {s:?} must map to {}",
                    b.canonical
                );
            }
        }
    }

    #[test]
    fn book_order_matches_the_bundled_seed_orders() {
        // The bundled curated verses hard-code book_order; the canon must agree,
        // or a downloaded corpus would sort differently from the starter set.
        for t in bundled_translations() {
            for v in t.verses {
                assert_eq!(
                    book_order(v.book),
                    Some(v.book_order),
                    "{} order disagrees between seed and canon",
                    v.book
                );
            }
        }
    }

    #[test]
    fn new_books_localize_and_resolve() {
        // A book that only exists in the canon after the C1 extension.
        assert_eq!(resolve_book("Josva"), Some("Joshua".into()));
        assert_eq!(book_display("Joshua", "no"), "Josva");
        assert_eq!(book_display("Joshua", "en"), "Joshua");
        assert_eq!(resolve_book("1 Sam"), Some("1 Samuel".into()));
        assert_eq!(resolve_book("Åpenbaringen"), Some("Revelation".into()));
    }
}
