//! Spor C (C1/C2) — the full-corpus Bible downloader.
//!
//! The bundled translations (`services::bible::bundled_translations`) are a
//! curated starter set — the handful of passages a church actually projects —
//! so a fresh, offline install already has scripture and the ~32 MB of four
//! complete Bibles never rides inside the binary. This module is the other half
//! the README always promised: on demand it fetches a complete public-domain
//! translation, verifies it, and installs it into the ordinary
//! `bible_translation`/`bible_verse` tables through [`BibleRepo`].
//!
//! ## Source and trust
//!
//! The corpora come from `scrollmapper/bible_databases` (repository code MIT;
//! the underlying texts are public domain). We fetch individual JSON files from
//! **a pinned commit** ([`SCROLLMAPPER_REF`]) rather than a moving branch — a
//! commit ref on `raw.githubusercontent.com` is immutable and content-addressed
//! — and verify each download against a **pinned SHA-256** ([`CorpusSource::sha256`])
//! before a single verse is parsed. A moved ref, a truncated transfer or a
//! substituted file all fail the checksum and are rejected, never seeded.
//!
//! ⚠️ Only the specific files verified as public domain are listed here. In
//! particular this is NOT CrossWire's annotated KJV (GPL): the KJV/ASV/Bibelen
//! 1930/Studentmållagsbibelen files below carry plain verse text.
//!
//! ## Corpus JSON shape
//!
//! ```json
//! { "translation": "...",
//!   "books": [ { "name": "Revelation of John",
//!                "chapters": [ { "chapter": 1,
//!                                "verses": [ { "verse": 1, "text": "..." } ] } ] } ] }
//! ```
//!
//! `books[].name` is a full English name in the source's own spelling (numbered
//! books are roman-numeralled, Revelation is "Revelation of John"), which
//! [`crate::services::bible::canonical_for_source_name`] maps to our canonical
//! `book`/`book_order`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::db::models::BibleTranslation;
use crate::db::repositories::BibleRepo;
use crate::error::{AppError, AppResult};
use crate::services::bible::canonical_for_source_name;

/// The scrollmapper commit every corpus URL and checksum is pinned to.
///
/// Immutable: `raw.githubusercontent.com/<owner>/<repo>/<sha>/…` serves the
/// exact bytes committed at this SHA, so the checksums below are stable forever.
/// Bump this ref and the four checksums together, never one without the other.
pub const SCROLLMAPPER_REF: &str = "e1b254cef86d0e65b1a5d1a94b8b112d0f296a2c";

/// The Tauri event name the download command emits progress on.
pub const PROGRESS_EVENT: &str = "bible://download-progress";

/// One downloadable public-domain translation, pinned to [`SCROLLMAPPER_REF`].
pub struct CorpusSource {
    /// Our translation code. Shared with the bundled seed where they overlap
    /// (`KJV`, `NB1930`) so downloading upgrades the starter set in place.
    pub code: &'static str,
    /// Display name stored on the translation row.
    pub name: &'static str,
    /// ISO-639-1 language code.
    pub language: &'static str,
    /// The scrollmapper `formats/json/<file>` filename.
    pub file: &'static str,
    /// SHA-256 of the file at [`SCROLLMAPPER_REF`], lowercase hex.
    pub sha256: &'static str,
    /// The file's exact byte length at the pinned ref — the progress total when
    /// the server does not send a `Content-Length`.
    pub bytes: i64,
}

impl CorpusSource {
    /// The immutable raw URL for this corpus at the pinned commit.
    pub fn url(&self) -> String {
        format!(
            "https://raw.githubusercontent.com/scrollmapper/bible_databases/{SCROLLMAPPER_REF}/formats/json/{}",
            self.file
        )
    }
}

/// The corpora we offer. All four are complete 66-book Bibles (~31k verses each,
/// ~8 MB of JSON), verified public domain:
///   * KJV / ASV — English.
///   * NB1930 — Bibelen 1930 (Norwegian bokmål; scrollmapper "Norsk").
///   * NorSMB — Studentmållagsbibelen 1921 (Norwegian nynorsk).
///
/// `KJV`/`NB1930` reuse the bundled seed's codes, so downloading them replaces
/// the curated starter verses with the full text under the same translation id.
pub const CATALOG: &[CorpusSource] = &[
    CorpusSource {
        code: "KJV",
        name: "King James Version",
        language: "en",
        file: "KJV.json",
        sha256: "f0b09dc49dfb97bb84f03aae1fbf026485048c3cab31a7a41017e2d86ac1d11c",
        bytes: 8_400_187,
    },
    CorpusSource {
        code: "ASV",
        name: "American Standard Version",
        language: "en",
        file: "ASV.json",
        sha256: "602445e22c280a682ac4c489117ead179271f5ee50a78ee4531b249c71e7ce99",
        bytes: 8_401_279,
    },
    CorpusSource {
        code: "NB1930",
        name: "Bibelen 1930",
        language: "no",
        file: "Norsk.json",
        sha256: "9c3ed3d9ec651937895f0d2a903932b5807071683402dde1c45f27ccf4e1fccb",
        bytes: 8_024_090,
    },
    CorpusSource {
        code: "NorSMB",
        name: "Studentmållagsbibelen 1921",
        language: "no",
        file: "NorSMB.json",
        sha256: "a0678c7b88790e9dec03195ff34068ead44d4483e767d8d50e96bce2cd08a737",
        bytes: 8_039_272,
    },
];

/// Look up a corpus by its translation code.
pub fn source_for_code(code: &str) -> Option<&'static CorpusSource> {
    CATALOG.iter().find(|s| s.code == code)
}

/// A translation the operator can install, with how much of it is present now.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/AvailableTranslation.ts")]
#[serde(rename_all = "camelCase")]
pub struct AvailableTranslation {
    pub code: String,
    pub name: String,
    pub language: String,
    /// Approximate download size in bytes (the pinned file length).
    #[ts(type = "number")]
    pub approx_bytes: i64,
    /// Verses of this code already in the library: 0 = not installed, a small
    /// number = the bundled starter set, ~31k = the full corpus.
    #[ts(type = "number")]
    pub installed_verses: i64,
}

/// Which phase a running download is in, for the progress event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/BibleDownloadPhase.ts")]
pub enum BibleDownloadPhase {
    Downloading,
    Verifying,
    Installing,
    Done,
}

/// A progress tick emitted on [`PROGRESS_EVENT`] while a corpus downloads.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/bindings/BibleDownloadProgress.ts")]
pub struct BibleDownloadProgress {
    /// The translation code being installed.
    pub code: String,
    pub phase: BibleDownloadPhase,
    /// Bytes downloaded so far (only meaningful in the `downloading` phase).
    #[ts(type = "number")]
    pub downloaded: i64,
    /// Total bytes expected.
    #[ts(type = "number")]
    pub total: i64,
}

impl BibleDownloadProgress {
    fn new(code: &str, phase: BibleDownloadPhase, downloaded: i64, total: i64) -> Self {
        Self {
            code: code.to_string(),
            phase,
            downloaded,
            total,
        }
    }
}

/// One verse ready to insert: the canonical book, its order, and the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVerse {
    pub book: &'static str,
    pub book_order: i64,
    pub chapter: i64,
    pub verse: i64,
    pub text: String,
}

// ── The scrollmapper JSON shape (only the fields we consume) ─────────────────

#[derive(Debug, Deserialize)]
struct RawBible {
    books: Vec<RawBook>,
}

#[derive(Debug, Deserialize)]
struct RawBook {
    name: String,
    chapters: Vec<RawChapter>,
}

#[derive(Debug, Deserialize)]
struct RawChapter {
    chapter: i64,
    verses: Vec<RawVerse>,
}

#[derive(Debug, Deserialize)]
struct RawVerse {
    verse: i64,
    text: String,
}

/// Verify a downloaded corpus against its pinned SHA-256. This runs *before*
/// parsing, so a corrupt or substituted file never reaches the database.
pub fn verify_checksum(bytes: &[u8], expected_hex: &str) -> AppResult<()> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let got = hex::encode(hasher.finalize());
    if got.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "sjekksum for bibelnedlastingen stemte ikke: forventet {expected_hex}, fikk {got}"
        )))
    }
}

/// Parse a scrollmapper corpus into canonical verse rows.
///
/// Every `books[].name` must map to our 66-book canon; an unknown book aborts
/// the whole install rather than seeding a book with no canonical order.
pub fn parse_corpus(bytes: &[u8]) -> AppResult<Vec<ParsedVerse>> {
    let raw: RawBible = serde_json::from_slice(bytes)?;
    let mut out = Vec::new();
    for book in raw.books {
        let (canonical, order) = canonical_for_source_name(&book.name).ok_or_else(|| {
            AppError::Validation(format!("ukjent bok i bibelkorpuset: {}", book.name))
        })?;
        for ch in book.chapters {
            let chapter = ch.chapter;
            for v in ch.verses {
                out.push(ParsedVerse {
                    book: canonical,
                    book_order: order,
                    chapter,
                    verse: v.verse,
                    text: v.text,
                });
            }
        }
    }
    if out.is_empty() {
        return Err(AppError::Validation(
            "bibelkorpuset inneholder ingen vers".into(),
        ));
    }
    Ok(out)
}

/// Download a corpus body, reporting byte progress as it streams.
///
/// Uses `Response::chunk` (available without reqwest's `stream` feature) so the
/// operator sees movement on an 8 MB file instead of a frozen spinner.
async fn fetch_bytes<F>(url: &str, expected_total: i64, on_progress: F) -> AppResult<Vec<u8>>
where
    F: Fn(i64, i64),
{
    let mut resp = reqwest::get(url)
        .await
        .map_err(|e| AppError::Internal(format!("bibelnedlasting mislyktes: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "bibelnedlasting mislyktes: HTTP {}",
            resp.status()
        )));
    }
    let total = resp
        .content_length()
        .map(|c| c as i64)
        .unwrap_or(expected_total);
    let mut buf: Vec<u8> = Vec::with_capacity(total.max(0) as usize);
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| AppError::Internal(format!("bibelnedlasting avbrutt: {e}")))?
    {
        buf.extend_from_slice(&chunk);
        on_progress(buf.len() as i64, total);
    }
    Ok(buf)
}

/// Download → checksum → parse → install one corpus, driving `on_progress`
/// through every phase. Idempotent: re-running replaces the translation's verses
/// cleanly (see [`BibleRepo::replace_translation`]), and FTS stays consistent.
pub async fn download_and_install<F>(
    pool: &SqlitePool,
    source: &CorpusSource,
    on_progress: F,
) -> AppResult<BibleTranslation>
where
    F: Fn(BibleDownloadProgress),
{
    on_progress(BibleDownloadProgress::new(
        source.code,
        BibleDownloadPhase::Downloading,
        0,
        source.bytes,
    ));
    let bytes = fetch_bytes(&source.url(), source.bytes, |d, t| {
        on_progress(BibleDownloadProgress::new(
            source.code,
            BibleDownloadPhase::Downloading,
            d,
            t,
        ));
    })
    .await?;

    on_progress(BibleDownloadProgress::new(
        source.code,
        BibleDownloadPhase::Verifying,
        source.bytes,
        source.bytes,
    ));
    verify_checksum(&bytes, source.sha256)?;

    on_progress(BibleDownloadProgress::new(
        source.code,
        BibleDownloadPhase::Installing,
        source.bytes,
        source.bytes,
    ));
    let parsed = parse_corpus(&bytes)?;
    let installed = BibleRepo::new(pool)
        .replace_translation(source.code, source.name, source.language, &parsed)
        .await?;

    on_progress(BibleDownloadProgress::new(
        source.code,
        BibleDownloadPhase::Done,
        source.bytes,
        source.bytes,
    ));
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny scrollmapper-shaped corpus that exercises the three name shapes
    /// the real files use: a plain name, a roman-numeral numbered book, and
    /// "Revelation of John" — plus Psalms, whose *number* (19) matters.
    const SYNTHETIC: &str = r#"{
        "translation": "TEST",
        "books": [
            { "name": "Psalms", "chapters": [
                { "chapter": 23, "verses": [
                    { "verse": 1, "text": "The LORD is my shepherd; I shall not want." }
                ] } ] },
            { "name": "I Corinthians", "chapters": [
                { "chapter": 13, "verses": [
                    { "verse": 4, "text": "Charity suffereth long, and is kind" },
                    { "verse": 13, "text": "And now abideth faith, hope, charity" }
                ] } ] },
            { "name": "Revelation of John", "chapters": [
                { "chapter": 22, "verses": [
                    { "verse": 21, "text": "The grace of our Lord Jesus Christ be with you all. Amen." }
                ] } ] }
        ]
    }"#;

    #[test]
    fn parses_synthetic_corpus_into_canonical_rows() {
        let rows = parse_corpus(SYNTHETIC.as_bytes()).unwrap();
        assert_eq!(rows.len(), 4);

        // Plain name + its number preserved.
        let ps = &rows[0];
        assert_eq!(
            (ps.book, ps.book_order, ps.chapter, ps.verse),
            ("Psalms", 19, 23, 1)
        );

        // Roman-numeral numbered book canonicalized.
        assert_eq!(rows[1].book, "1 Corinthians");
        assert_eq!(rows[1].book_order, 46);
        assert_eq!(rows[2].verse, 13);

        // "Revelation of John" → our canonical "Revelation".
        let rev = &rows[3];
        assert_eq!(
            (rev.book, rev.book_order, rev.chapter, rev.verse),
            ("Revelation", 66, 22, 21)
        );
    }

    #[test]
    fn unknown_book_aborts_the_parse() {
        let json = r#"{ "books": [ { "name": "Tobit", "chapters": [
            { "chapter": 1, "verses": [ { "verse": 1, "text": "…" } ] } ] } ] }"#;
        let err = parse_corpus(json.as_bytes()).unwrap_err();
        assert!(matches!(err, AppError::Validation(m) if m.contains("Tobit")));
    }

    #[test]
    fn empty_corpus_is_rejected() {
        let json = r#"{ "books": [] }"#;
        assert!(matches!(
            parse_corpus(json.as_bytes()),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(parse_corpus(b"not json"), Err(AppError::Json(_))));
    }

    #[test]
    fn checksum_accepts_the_matching_digest() {
        // SHA-256 of "abc" is a well-known KAT.
        let want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_checksum(b"abc", want).is_ok());
        // Case-insensitive, so an upper-case pin still matches.
        assert!(verify_checksum(b"abc", &want.to_uppercase()).is_ok());
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let wrong = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(matches!(
            verify_checksum(b"abc", wrong),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn catalog_is_well_formed() {
        assert!(!CATALOG.is_empty());
        for s in CATALOG {
            assert_eq!(s.sha256.len(), 64, "{} sha must be 64 hex chars", s.code);
            assert!(
                s.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{} sha must be hex",
                s.code
            );
            assert!(s.bytes > 0, "{} needs a positive pinned size", s.code);
            assert!(
                s.url().contains(SCROLLMAPPER_REF),
                "{} url must be pinned to the ref",
                s.code
            );
            assert!(s.url().ends_with(s.file));
            assert!(source_for_code(s.code).is_some());
        }
        // The Norwegian bokmål corpus reuses the bundled seed's code so a
        // download upgrades the starter set in place rather than duplicating it.
        assert!(source_for_code("NB1930").is_some());
        assert!(source_for_code("KJV").is_some());
        assert!(source_for_code("does-not-exist").is_none());
    }
}
