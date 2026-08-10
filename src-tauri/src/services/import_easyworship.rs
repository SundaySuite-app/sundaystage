//! Shoulders program, Spor B2 — EasyWorship 6/8 song importer.
//!
//! EasyWorship stores a song library across several plain SQLite files in
//! `…/Databases/Data/`. Two of them matter here (verified against the owner's
//! real 223-song library this session):
//!
//!   * **`Songs.db`** → table `song` — one row per song:
//!     `rowid`, `title`, `author`, `copyright`, `administrator`,
//!     `reference_number`, … The text columns are declared
//!     `COLLATE UTF8_U_CI` — a *custom* collation registered only by
//!     EasyWorship itself. **Any comparison or sort touching those columns
//!     (`WHERE title = …`, `ORDER BY title`, even `WHERE reference_number <> ''`)
//!     raises `no such collation sequence: UTF8_U_CI` when opened by anyone
//!     else.** A plain projection (`SELECT title, author, …` with no
//!     `WHERE`/`ORDER BY` on those columns) is safe, so this reader pulls the
//!     rows raw and does every filter/compare in Rust.
//!   * **`SongWords.db`** → table `word` — lyrics as RTF in a `words` column,
//!     keyed by `song_id` = the `Songs.db` `rowid` (unique index, one row per
//!     song). The RTF begins `{\rtf1\ansi…}`; lines are `…\par`-terminated and
//!     stanzas are separated by an empty `\par`.
//!
//! Everything user-visible lands through the universal [`apply_formatted_song`]
//! seam: [`convert_song`] turns one joined EasyWorship row into a
//! [`FormattedSong`] (title/language/sections/arrangement) plus the metadata
//! that `FormattedSong` cannot carry — see [`ConvertedSong`]. Lyric decoding is
//! delegated to the shared [`rtf::rtf_to_text`] (B1) and section splitting to the
//! shared [`heuristic_format`], so this module is small and the hard parts stay
//! in one place. The command that reads the files and writes the DB lives in
//! `commands::import`.
//!
//! ## Why sqlx read-only, not a second SQLite crate (rusqlite)
//!
//! The program sketch named `rusqlite` for B2, but this codebase already links
//! SQLite: `sqlx`'s `sqlite` feature pulls `libsqlite3-sys` (0.30.1 in the lock
//! today) and statically bundles the C engine into the binary. Adding
//! `rusqlite` with `bundled` would pull *its own* `libsqlite3-sys` — and unless
//! it resolves to the exact same version sqlx uses, Cargo links **two** copies
//! of the SQLite C library and the build fails with duplicate symbols. (The
//! local Cargo cache already holds a second, incompatible `libsqlite3-sys`
//! 0.37.0 from another project — proof the mismatch is not hypothetical.)
//! Pinning `rusqlite` to whatever version happens to shadow sqlx's transitive
//! `libsqlite3-sys` would work today but is a maintenance landmine: the next
//! sqlx bump silently desyncs it.
//!
//! sqlx already exposes exactly what reading a foreign file needs —
//! `SqliteConnectOptions::new().read_only(true).immutable(true)
//! .create_if_missing(false)` — reusing the engine that is already vetted and
//! shipped. `immutable(true)` also means SQLite never writes a `-wal`/`-shm`
//! sidecar into the owner's EasyWorship folder. So the read-only path here opens
//! a **separate, transient connection** for each foreign file; the application's
//! own pool (`state.db.pool`) is never touched. The custom-collation trap is
//! identical for either library, so rusqlite would buy nothing there either.
//! Net: zero new dependencies, zero new compiled crates, no double-link risk.
//!
//! Everything in the pure layer ([`convert_song`] and its helpers) never panics
//! — malformed rows degrade to a best-effort song, matching the house style of
//! `song_import` and `rtf`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::{ConnectOptions, Connection, Row, SqliteConnection};

use crate::error::{AppError, AppResult};
use crate::services::ai::lyric_format::{heuristic_format, FormattedSong};
use crate::services::rtf;

/// One EasyWorship `song` row joined with its lyrics RTF from `word`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EasyWorshipSong {
    pub rowid: i64,
    pub title: String,
    pub author: Option<String>,
    pub copyright: Option<String>,
    pub administrator: Option<String>,
    pub reference_number: Option<String>,
    /// Raw RTF from `SongWords.word.words` (`None` when the song has no words
    /// row, e.g. a title-only entry).
    pub words_rtf: Option<String>,
}

/// The result of converting one EasyWorship song: the [`FormattedSong`] that
/// flows through [`apply_formatted_song`](crate::services::ai::lyric_format::apply_formatted_song),
/// plus the metadata `FormattedSong` has no field for and which is applied to
/// the song row at creation time (`ccli_song_id`, `copyright_notice`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConvertedSong {
    pub formatted: FormattedSong,
    /// Composed from `author` + `copyright` + `administrator` (see
    /// [`compose_copyright`]). EasyWorship keeps the author separate but this
    /// app has no reachable author field on `SongInput`, so folding it into the
    /// copyright notice preserves it rather than dropping it on migration.
    pub copyright_notice: Option<String>,
    /// Set only when `reference_number` looks like a CCLI song number
    /// (see [`ccli_from_reference`]).
    pub ccli_song_id: Option<String>,
}

/// Trim a possibly-`None` string, returning `None` for empty/whitespace-only.
fn clean_opt(s: Option<&str>) -> Option<String> {
    let t = s?.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Convert one joined EasyWorship row into a [`ConvertedSong`].
///
/// The lyrics RTF is decoded with the shared [`rtf::rtf_to_text`] (which turns
/// each `\par` into a newline and keeps the empty-paragraph stanza breaks), then
/// split into sections by the shared [`heuristic_format`] — the same
/// blank-line-separated-stanzas-become-sections logic every other importer uses,
/// so section labelling, repeated-block-as-chorus and arrangement all behave
/// identically. The song title always comes from the authoritative `song.title`
/// column, overriding the heuristic's (absent) title guess.
pub fn convert_song(song: &EasyWorshipSong) -> ConvertedSong {
    let rtf_src = song.words_rtf.as_deref().unwrap_or("");
    let lyrics = rtf::rtf_to_text(rtf_src);

    let mut formatted = heuristic_format(&lyrics);
    // The database title is authoritative — never the first lyric line.
    formatted.title_suggestion = clean_opt(Some(&song.title));

    ConvertedSong {
        formatted,
        copyright_notice: compose_copyright(
            song.author.as_deref(),
            song.copyright.as_deref(),
            song.administrator.as_deref(),
        ),
        ccli_song_id: ccli_from_reference(song.reference_number.as_deref()),
    }
}

/// Build a single copyright-notice string from the three EasyWorship metadata
/// columns. `administrator` is rendered with the CCLI-conventional `Admin. by`
/// prefix so the block reads naturally. Returns `None` when all three are empty.
fn compose_copyright(
    author: Option<&str>,
    copyright: Option<&str>,
    administrator: Option<&str>,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(a) = clean_opt(author) {
        parts.push(a);
    }
    if let Some(c) = clean_opt(copyright) {
        parts.push(c);
    }
    if let Some(adm) = clean_opt(administrator) {
        parts.push(format!("Admin. by {adm}"));
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}

/// Best-effort CCLI song-number detection from `reference_number`. Churches use
/// this free-text field for different things (hymnal numbers, catalogue codes,
/// CCLI numbers), so this is deliberately conservative:
///
///   * a value containing `CCLI` (any case) → its digit run, if 4–8 digits;
///   * otherwise a bare all-digit value of 4–8 digits (CCLI song ids sit in
///     that range) → itself;
///   * everything else → `None` (a 1–3 digit hymnal number is *not* claimed as
///     a CCLI id).
fn ccli_from_reference(reference: Option<&str>) -> Option<String> {
    let raw = clean_opt(reference)?;
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    let plausible = |d: &str| (4..=8).contains(&d.len());

    if raw.to_lowercase().contains("ccli") {
        return plausible(&digits).then_some(digits);
    }
    if raw.chars().all(|c| c.is_ascii_digit()) && plausible(&raw) {
        return Some(raw);
    }
    None
}

/// Given a folder the user pointed at, locate `Songs.db` + `SongWords.db`.
/// Accepts either the EasyWorship `…/Databases/Data` folder directly, or its
/// parent (a `Data` subfolder is then tried). Returns a validation error naming
/// the folder when neither layout matches.
pub fn locate_databases(folder: &Path) -> AppResult<(PathBuf, PathBuf)> {
    for base in [folder.to_path_buf(), folder.join("Data")] {
        let songs = base.join("Songs.db");
        let words = base.join("SongWords.db");
        if songs.is_file() && words.is_file() {
            return Ok((songs, words));
        }
    }
    Err(AppError::Validation(format!(
        "Fant ikke Songs.db og SongWords.db i {}",
        folder.display()
    )))
}

/// Open a foreign SQLite file strictly read-only and immutable, so we never lock
/// it, never create `-wal`/`-shm` sidecars in the owner's folder, and never risk
/// writing to a file we don't own.
async fn open_readonly(path: &Path) -> AppResult<SqliteConnection> {
    if !path.is_file() {
        return Err(AppError::Validation(format!(
            "finner ikke databasefilen {}",
            path.display()
        )));
    }
    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(true)
        .create_if_missing(false);
    Ok(opts.connect().await?)
}

/// Read the `word` table into a `song_id → RTF` map. The `words` column is
/// declared with a custom `rtf` type affinity but stores text; a value that is
/// unexpectedly a blob is decoded lossily rather than failing the whole import.
async fn read_words(words_db: &Path) -> AppResult<HashMap<i64, String>> {
    let mut conn = open_readonly(words_db).await?;
    let rows = sqlx::query("SELECT song_id, words FROM word")
        .fetch_all(&mut conn)
        .await?;
    conn.close().await.ok();

    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let Some(song_id) = row.try_get::<Option<i64>, _>("song_id")? else {
            continue; // orphan word row with no song key
        };
        let words: Option<String> = match row.try_get::<Option<String>, _>("words") {
            Ok(v) => v,
            Err(_) => row
                .try_get::<Option<Vec<u8>>, _>("words")
                .ok()
                .flatten()
                .map(|b| String::from_utf8_lossy(&b).into_owned()),
        };
        if let Some(w) = words {
            map.insert(song_id, w);
        }
    }
    Ok(map)
}

/// Read every song from `Songs.db`, joined with its lyrics from `SongWords.db`.
///
/// The `song` table's text columns carry the custom `UTF8_U_CI` collation, so
/// this issues a bare `SELECT` with no `WHERE`/`ORDER BY` on those columns (a
/// projection never invokes the collation) and leaves all ordering/filtering to
/// the caller in Rust.
pub async fn read_songs(songs_db: &Path, words_db: &Path) -> AppResult<Vec<EasyWorshipSong>> {
    let words = read_words(words_db).await?;

    let mut conn = open_readonly(songs_db).await?;
    let rows = sqlx::query(
        "SELECT rowid, title, author, copyright, administrator, reference_number FROM song",
    )
    .fetch_all(&mut conn)
    .await?;
    conn.close().await.ok();

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let rowid: i64 = row.try_get("rowid")?;
        out.push(EasyWorshipSong {
            rowid,
            title: row
                .try_get::<Option<String>, _>("title")?
                .unwrap_or_default(),
            author: row.try_get("author")?,
            copyright: row.try_get("copyright")?,
            administrator: row.try_get("administrator")?,
            reference_number: row.try_get("reference_number")?,
            words_rtf: words.get(&rowid).cloned(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── clean_opt ────────────────────────────────────────────────────────────
    #[test]
    fn clean_opt_trims_and_nulls_empty() {
        assert_eq!(clean_opt(Some("  Lina  ")), Some("Lina".to_string()));
        assert_eq!(clean_opt(Some("   ")), None);
        assert_eq!(clean_opt(Some("")), None);
        assert_eq!(clean_opt(None), None);
    }

    // ── compose_copyright ────────────────────────────────────────────────────
    #[test]
    fn compose_copyright_joins_present_parts() {
        assert_eq!(
            compose_copyright(Some("Lina Sandell"), Some("© 1865"), Some("Public Domain")),
            Some("Lina Sandell\n© 1865\nAdmin. by Public Domain".to_string())
        );
        assert_eq!(
            compose_copyright(None, Some("© 2020 Church"), None),
            Some("© 2020 Church".to_string())
        );
        assert_eq!(compose_copyright(Some("  "), None, Some("")), None);
        assert_eq!(compose_copyright(None, None, None), None);
    }

    // ── ccli_from_reference ──────────────────────────────────────────────────
    #[test]
    fn ccli_detection_is_conservative() {
        assert_eq!(ccli_from_reference(Some("1234567")), Some("1234567".into()));
        assert_eq!(
            ccli_from_reference(Some("CCLI Song #7654321")),
            Some("7654321".into())
        );
        assert_eq!(
            ccli_from_reference(Some("  ccli: 44444 ")),
            Some("44444".into())
        );
        // Too short → likely a hymnal number, not CCLI.
        assert_eq!(ccli_from_reference(Some("42")), None);
        // Non-numeric catalogue code.
        assert_eq!(ccli_from_reference(Some("N-13a")), None);
        assert_eq!(ccli_from_reference(Some("")), None);
        assert_eq!(ccli_from_reference(None), None);
    }

    // ── convert_song (RTF → sections via the shared decoder + heuristic) ──────

    /// Build EasyWorship-shaped RTF: a realistic header (font/colour tables and
    /// the `{\*\pnseclvl…}` list groups EasyWorship emits, all of which must be
    /// stripped) followed by `\par`-terminated lines. Blank stanza breaks are an
    /// empty `\par`. Norwegian letters are written as the `\'xx` CP1252 hex
    /// escapes EasyWorship actually stores.
    fn ew_rtf(body: &str) -> String {
        format!(
            "{{\\rtf1\\ansi\\ansicpg1252\\deff0\\deflang1044\
             {{\\fonttbl{{\\f0\\fswiss\\fcharset0 Arial;}}{{\\f1\\froman Times New Roman;}}}}\
             {{\\colortbl ;\\red0\\green0\\blue0;\\red255\\green255\\blue255;}}\
             {{\\*\\pnseclvl1\\pnucrm\\pnstart1\\pnindent720\\pnhang{{\\pntxta .}}}}\
             \\viewkind4\\uc1\\pard\\f1\\fs48 {body}}}"
        )
    }

    /// One `\par`-terminated line in the EasyWorship body, with `æøå` as CP1252
    /// hex escapes so the test also exercises the decoder's codepage path.
    fn line(text: &str) -> String {
        let escaped = text
            .replace('æ', "\\'e6")
            .replace('ø', "\\'f8")
            .replace('å', "\\'e5")
            .replace('Å', "\\'c5");
        format!("{escaped}\\par ")
    }

    #[test]
    fn convert_song_decodes_rtf_and_splits_stanzas() {
        // Two stanzas separated by an empty \par, plus Norwegian letters.
        let body = format!(
            "{}{}{}{}{}{}",
            line("Deg være ære"),
            line("evige Gud"),
            line(""), // blank line → stanza break
            line("Måne og sol"),
            line("skyer og vind"),
            line("de synger en sang"),
        );
        let song = EasyWorshipSong {
            rowid: 1,
            title: "Deg være ære".to_string(),
            author: Some("Edmond Budry".to_string()),
            copyright: Some("Offentlig eiendom".to_string()),
            administrator: None,
            reference_number: None,
            words_rtf: Some(ew_rtf(&body)),
        };

        let out = convert_song(&song);

        // Title comes from the DB column, language detected from the lyrics.
        assert_eq!(
            out.formatted.title_suggestion.as_deref(),
            Some("Deg være ære")
        );
        assert_eq!(out.formatted.language, "no");

        // Two blank-line-separated stanzas → two sections.
        assert_eq!(out.formatted.sections.len(), 2);
        assert_eq!(out.formatted.sections[0].label, "verse_1");
        assert_eq!(out.formatted.sections[0].lyrics, "Deg være ære\nevige Gud");
        assert_eq!(
            out.formatted.sections[1].lyrics,
            "Måne og sol\nskyer og vind\nde synger en sang"
        );
        // No RTF control text leaked (font names, colour separators, list markup).
        for s in &out.formatted.sections {
            assert!(
                !s.lyrics.contains('\\'),
                "control words leaked: {:?}",
                s.lyrics
            );
            assert!(!s.lyrics.contains("Arial"), "font table leaked");
            assert!(!s.lyrics.contains(';'), "colour table leaked");
        }

        assert_eq!(
            out.copyright_notice.as_deref(),
            Some("Edmond Budry\nOffentlig eiendom")
        );
        assert_eq!(out.ccli_song_id, None);
    }

    #[test]
    fn convert_song_detects_section_headers_and_repeated_chorus() {
        // Header lines ("Vers 1"/"Refreng") become labels; the repeated block is
        // collapsed to one chorus referenced twice.
        let body = format!(
            "{}{}{}{}{}{}{}{}{}{}{}",
            line("Vers 1"),
            line("En strofe her"),
            line(""),
            line("Refreng"),
            line("Halleluja syng"),
            line(""),
            line("Vers 2"),
            line("En annen strofe"),
            line(""),
            line("Refreng"),
            line("Halleluja syng"),
        );
        let song = EasyWorshipSong {
            rowid: 2,
            title: "Med headers".to_string(),
            words_rtf: Some(ew_rtf(&body)),
            ..Default::default()
        };
        let out = convert_song(&song);
        let labels: Vec<&str> = out
            .formatted
            .sections
            .iter()
            .map(|s| s.label.as_str())
            .collect();
        assert!(labels.contains(&"chorus"), "labels: {labels:?}");
        // chorus referenced twice in the arrangement, stored once as a section
        assert_eq!(
            out.formatted
                .arrangement
                .iter()
                .filter(|l| *l == "chorus")
                .count(),
            2
        );
        assert_eq!(
            out.formatted
                .sections
                .iter()
                .filter(|s| s.label == "chorus")
                .count(),
            1
        );
    }

    #[test]
    fn convert_song_ccli_and_admin_flow_through() {
        let song = EasyWorshipSong {
            rowid: 3,
            title: "Full metadata".to_string(),
            author: Some("Stuart Townend".to_string()),
            copyright: Some("© 1995 Thankyou Music".to_string()),
            administrator: Some("Capitol CMG".to_string()),
            reference_number: Some("1558110".to_string()),
            words_rtf: Some(ew_rtf(&line("How deep the Father's love"))),
        };
        let out = convert_song(&song);
        assert_eq!(out.ccli_song_id.as_deref(), Some("1558110"));
        assert_eq!(
            out.copyright_notice.as_deref(),
            Some("Stuart Townend\n© 1995 Thankyou Music\nAdmin. by Capitol CMG")
        );
        assert_eq!(out.formatted.language, "en");
    }

    #[test]
    fn convert_song_missing_words_is_a_titled_empty_song() {
        let song = EasyWorshipSong {
            rowid: 4,
            title: "Bare tittel".to_string(),
            words_rtf: None,
            ..Default::default()
        };
        let out = convert_song(&song);
        assert_eq!(
            out.formatted.title_suggestion.as_deref(),
            Some("Bare tittel")
        );
        assert!(out.formatted.sections.is_empty());
        assert!(!out.formatted.warnings.is_empty());
    }

    // ── locate_databases ─────────────────────────────────────────────────────
    #[test]
    fn locate_databases_finds_direct_and_data_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        // Neither file yet → error.
        assert!(locate_databases(dir.path()).is_err());

        // Direct layout.
        std::fs::write(dir.path().join("Songs.db"), b"x").unwrap();
        std::fs::write(dir.path().join("SongWords.db"), b"x").unwrap();
        let (s, w) = locate_databases(dir.path()).unwrap();
        assert!(s.ends_with("Songs.db") && w.ends_with("SongWords.db"));

        // Parent-of-Data layout.
        let parent = tempfile::tempdir().unwrap();
        let data = parent.path().join("Data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("Songs.db"), b"x").unwrap();
        std::fs::write(data.join("SongWords.db"), b"x").unwrap();
        let (s, _) = locate_databases(parent.path()).unwrap();
        assert!(s.starts_with(&data));
    }

    // ── read_songs against a synthetic EasyWorship-shaped fixture ─────────────
    //
    // Builds two real SQLite files with the EasyWorship schema — including the
    // `title text COLLATE UTF8_U_CI` declaration and the `word.words` column's
    // custom `rtf` affinity — using a locally-registered stand-in collation, then
    // proves `read_songs` reads them back WITHOUT that collation registered. This
    // is the regression guard for "must not depend on UTF8_U_CI".

    async fn write_fixture(dir: &Path) -> (PathBuf, PathBuf) {
        use sqlx::sqlite::SqliteConnectOptions;

        let songs_path = dir.join("Songs.db");
        let words_path = dir.join("SongWords.db");

        // Songs.db — register a stand-in UTF8_U_CI so CREATE TABLE/INDEX with the
        // custom collation succeeds (exactly as EasyWorship's own process does).
        let mut songs: SqliteConnection = SqliteConnectOptions::new()
            .filename(&songs_path)
            .create_if_missing(true)
            .connect()
            .await
            .unwrap();
        register_nocase_collation(&mut songs).await;
        sqlx::query(
            "CREATE TABLE song (\
                rowid integer PRIMARY KEY AUTOINCREMENT NOT NULL,\
                title text NOT NULL COLLATE UTF8_U_CI,\
                author text COLLATE UTF8_U_CI,\
                copyright text COLLATE UTF8_U_CI,\
                administrator text COLLATE UTF8_U_CI,\
                reference_number text COLLATE UTF8_U_CI)",
        )
        .execute(&mut songs)
        .await
        .unwrap();
        sqlx::query("CREATE INDEX song_Index01 ON song(title)")
            .execute(&mut songs)
            .await
            .unwrap();
        for (title, author, copyright, admin, refno) in [
            (
                "Deg være ære",
                Some("Edmond Budry"),
                Some("Offentlig eiendom"),
                None,
                None,
            ),
            (
                "Nåde ufattelig",
                Some("John Newton"),
                Some("© 1779"),
                Some("Menigheten"),
                Some("22025"),
            ),
            ("Bare tittel", None, None, None, None),
        ] {
            sqlx::query(
                "INSERT INTO song (title, author, copyright, administrator, reference_number) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(title)
            .bind(author)
            .bind(copyright)
            .bind(admin)
            .bind(refno)
            .execute(&mut songs)
            .await
            .unwrap();
        }
        songs.close().await.ok();

        // SongWords.db — words column with the custom `rtf` affinity.
        let mut words: SqliteConnection = SqliteConnectOptions::new()
            .filename(&words_path)
            .create_if_missing(true)
            .connect()
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE word (\
                rowid integer PRIMARY KEY NOT NULL,\
                song_id integer,\
                words rtf,\
                slide_uids text)",
        )
        .execute(&mut words)
        .await
        .unwrap();
        sqlx::query("CREATE UNIQUE INDEX word_song_id ON word(song_id)")
            .execute(&mut words)
            .await
            .unwrap();
        // song_id 1 and 2 have words; song_id 3 (Bare tittel) has none.
        let rtf1 = ew_rtf(&format!("{}{}", line("Deg være ære"), line("evige Gud")));
        let rtf2 = ew_rtf(&format!(
            "{}{}{}{}",
            line("Nåde ufattelig"),
            line(""),
            line("så uendelig stor"),
            line("frelste en synder som meg"),
        ));
        for (song_id, rtf) in [(1i64, rtf1), (2, rtf2)] {
            sqlx::query("INSERT INTO word (song_id, words, slide_uids) VALUES (?, ?, ?)")
                .bind(song_id)
                .bind(rtf)
                .bind("")
                .execute(&mut words)
                .await
                .unwrap();
        }
        words.close().await.ok();

        (songs_path, words_path)
    }

    /// Register a permissive stand-in for EasyWorship's proprietary
    /// `UTF8_U_CI` collation on a fixture-building connection, so CREATE
    /// TABLE/INDEX statements that reference it succeed. The behaviour doesn't
    /// matter — the reader under test never invokes the collation.
    async fn register_nocase_collation(conn: &mut SqliteConnection) {
        // sqlx exposes the underlying sqlite handle via `lock_handle`.
        let mut handle = conn.lock_handle().await.unwrap();
        handle
            .create_collation("UTF8_U_CI", |a: &str, b: &str| {
                a.to_lowercase().cmp(&b.to_lowercase())
            })
            .unwrap();
    }

    #[tokio::test]
    async fn read_songs_reads_ew_fixture_without_the_custom_collation() {
        let dir = tempfile::tempdir().unwrap();
        let (songs_db, words_db) = write_fixture(dir.path()).await;

        // The reader opens the files fresh — it does NOT register UTF8_U_CI.
        let songs = read_songs(&songs_db, &words_db).await.unwrap();
        assert_eq!(songs.len(), 3);

        // Sort in Rust (never in SQL — that would need the collation).
        let by_id: HashMap<i64, &EasyWorshipSong> = songs.iter().map(|s| (s.rowid, s)).collect();

        let s1 = by_id[&1];
        assert_eq!(s1.title, "Deg være ære");
        assert_eq!(s1.author.as_deref(), Some("Edmond Budry"));
        assert!(s1.words_rtf.as_deref().unwrap().starts_with("{\\rtf1"));

        let s2 = by_id[&2];
        assert_eq!(s2.reference_number.as_deref(), Some("22025"));

        let s3 = by_id[&3];
        assert_eq!(s3.title, "Bare tittel");
        assert!(s3.words_rtf.is_none(), "title-only song has no words row");

        // End-to-end: convert reads through the RTF + heuristic pipeline.
        let c1 = convert_song(s1);
        assert_eq!(c1.formatted.sections.len(), 1);
        assert_eq!(c1.formatted.sections[0].lyrics, "Deg være ære\nevige Gud");

        let c2 = convert_song(s2);
        assert_eq!(c2.ccli_song_id.as_deref(), Some("22025"));
        assert_eq!(
            c2.copyright_notice.as_deref(),
            Some("John Newton\n© 1779\nAdmin. by Menigheten")
        );
        assert_eq!(c2.formatted.sections.len(), 2);
    }

    #[tokio::test]
    async fn ordering_by_collated_column_would_fail_but_read_songs_succeeds() {
        // Documents the trap: an ORDER BY on the collated column raises the
        // missing-collation error on our (collation-free) connection, while the
        // reader's projection-only query is fine.
        let dir = tempfile::tempdir().unwrap();
        let (songs_db, _words_db) = write_fixture(dir.path()).await;

        let mut conn = open_readonly(&songs_db).await.unwrap();
        let bad = sqlx::query("SELECT title FROM song ORDER BY title")
            .fetch_all(&mut conn)
            .await;
        assert!(bad.is_err(), "ORDER BY on a UTF8_U_CI column must fail");
        let ok = sqlx::query("SELECT rowid, title FROM song")
            .fetch_all(&mut conn)
            .await;
        assert!(ok.is_ok(), "a bare projection must not need the collation");
        conn.close().await.ok();
    }
}
