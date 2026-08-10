//! Shoulders program, Spor B4 — ProPresenter 6 (`.pro6`) song importer.
//!
//! A `.pro6` file is a single XML document rooted at `<RVPresentationDocument>`
//! (verified against the owner's 62 real Norwegian files this session):
//!
//!   * **CCLI metadata lives in root attributes** — `CCLISongTitle`,
//!     `CCLIAuthor`, `CCLICopyrightYear`, `CCLIPublisher`, `CCLISongNumber`.
//!   * **Slides** are `<RVDisplaySlide>` elements, grouped under
//!     `<array rvXMLIvarName="groups"><RVSlideGrouping name="…" uuid="…">`.
//!     Each slide's lyric text is a `<RVTextElement>` carrying
//!     `<NSString rvXMLIvarName="RTFData">` whose text is **base64-encoded RTF**
//!     (`{\rtf1\ansi\ansicpg1252\cocoartf…}` with `\'e5`/`\'f8`/`\'e6` for åøæ).
//!   * **Arrangements** are `<RVSongArrangement uuid="…">` whose
//!     `<array rvXMLIvarName="groupIDs">` lists group UUIDs (repeats allowed) in
//!     play order. `selectedArrangementID` on the root names the active one; it
//!     is empty in every owner file, so document order is the common path.
//!
//! Everything user-visible lands through the universal
//! [`apply_formatted_song`](crate::services::ai::lyric_format::apply_formatted_song)
//! seam: [`parse_pro6`] turns the document into a [`FormattedSong`] and
//! [`extract_metadata`] pulls the CCLI id + copyright notice that `FormattedSong`
//! has no field for. Both plug into `song_import` (the parse dispatch and the
//! command call `crate::services::song_import::parse_song` / `extra_metadata`),
//! reusing the single-file `import_song_file` path. Slide RTF decoding is
//! delegated to the shared [`rtf::rtf_base64_to_text`] (B1) and section
//! assembly to the shared [`song_import::finalize`], so the hard parts stay in
//! one place and this module is only the XML walk + ProPresenter semantics.
//!
//! ## Why `quick-xml`, not `roxmltree`
//!
//! `quick-xml` (MIT) is **already compiled into the tree** — `plist` pulls it in
//! via the Tauri macOS bundler (see `Cargo.lock`) — so declaring it a direct
//! dependency adds *no new compiled crate*, exactly the house-style rule the
//! other deps in `Cargo.toml` (parking_lot, sha2, hex, semver) follow.
//! `roxmltree` would have added a brand-new crate (plus `xmlparser`) for a
//! read-only walk `quick-xml`'s pull parser handles fine. The walk here needs
//! no DOM: a single streaming pass builds a tiny intermediate model.
//!
//! Everything here is pure and never panics: malformed XML degrades to a
//! best-effort song (empty + warned via `finalize`), never a crash — the live
//! path is holy.

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::services::ai::lyric_format::FormattedSong;
use crate::services::rtf;
use crate::services::song_import::{finalize, Block, ImportMetadata};

/// Parse a `.pro6` XML document into a [`FormattedSong`]: slides become sections
/// (named groups joined into one labelled section, unnamed groups split so each
/// slide is its own auto-numbered verse), ordered by the selected arrangement
/// when present and by document order otherwise. Title comes from
/// `CCLISongTitle`; when it is empty the caller falls back to the filename.
pub fn parse_pro6(content: &str) -> FormattedSong {
    document_to_song(&parse_document(content))
}

/// Extract the CCLI metadata that lives outside [`FormattedSong`] — the CCLI
/// song id and a composed copyright notice — from the root attributes only, so
/// this is a cheap companion to [`parse_pro6`] (it stops at the first element).
pub fn extract_metadata(content: &str) -> ImportMetadata {
    let mut reader = Reader::from_str(content);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"RVPresentationDocument" {
                    return ImportMetadata {
                        ccli_song_id: ccli_song_id(attr(&e, b"CCLISongNumber").as_deref()),
                        copyright_notice: compose_copyright(
                            attr(&e, b"CCLIAuthor").as_deref(),
                            attr(&e, b"CCLICopyrightYear").as_deref(),
                            attr(&e, b"CCLIPublisher").as_deref(),
                        ),
                    };
                }
                // The first element wasn't the ProPresenter root — not a .pro6.
                return ImportMetadata::default();
            }
            Ok(Event::Eof) | Err(_) => return ImportMetadata::default(),
            // Skip an XML declaration / comments / whitespace before the root.
            _ => {}
        }
    }
}

// ── Intermediate model ──────────────────────────────────────────────────────────

/// One `<RVSlideGrouping>`: a section name (may be empty) and each slide's
/// decoded plain text, in document order.
#[derive(Debug, Clone, PartialEq)]
struct Pro6Group {
    name: String,
    uuid: String,
    slides: Vec<String>,
}

/// One `<RVSongArrangement>`: an ordered list of group UUIDs (repeats allowed).
#[derive(Debug, Clone, PartialEq)]
struct Pro6Arrangement {
    uuid: String,
    group_ids: Vec<String>,
}

/// The parts of a `.pro6` we care about.
#[derive(Debug, Clone, PartialEq)]
struct Pro6Document {
    title: Option<String>,
    selected_arrangement_id: String,
    groups: Vec<Pro6Group>,
    arrangements: Vec<Pro6Arrangement>,
}

// ── Streaming XML walk ──────────────────────────────────────────────────────────

/// What the currently-open element means to us. Pushed on every `Start`, popped
/// on every `End`, so nesting stays balanced; `Empty` (self-closing) elements
/// touch nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Scope {
    Grouping,
    Slide,
    Arrangement,
    GroupIdsArray,
    RtfData,
    GroupId,
    Other,
}

/// Read an attribute's unescaped value, or `None` if absent/unreadable.
fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.try_get_attribute(key)
        .ok()
        .flatten()
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Walk the document once, building the intermediate model. On any XML error the
/// walk stops with whatever was *fully* collected so far (in-progress builders
/// are dropped), so truncated/broken input degrades to fewer/no sections rather
/// than panicking.
fn parse_document(content: &str) -> Pro6Document {
    let mut reader = Reader::from_str(content);

    let mut title: Option<String> = None;
    let mut selected = String::new();
    let mut groups: Vec<Pro6Group> = Vec::new();
    let mut arrangements: Vec<Pro6Arrangement> = Vec::new();

    let mut stack: Vec<Scope> = Vec::new();
    let mut cur_group: Option<Pro6Group> = None;
    let mut cur_slide: Option<Vec<String>> = None;
    let mut cur_arr: Option<Pro6Arrangement> = None;
    let mut text_buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let top = stack.last().copied().unwrap_or(Scope::Other);
                let ename = e.name();
                let scope = match ename.as_ref() {
                    b"RVPresentationDocument" => {
                        title = attr(&e, b"CCLISongTitle").filter(|s| !s.trim().is_empty());
                        selected = attr(&e, b"selectedArrangementID").unwrap_or_default();
                        Scope::Other
                    }
                    b"array" => {
                        if attr(&e, b"rvXMLIvarName").as_deref() == Some("groupIDs") {
                            Scope::GroupIdsArray
                        } else {
                            Scope::Other
                        }
                    }
                    b"RVSlideGrouping" => {
                        cur_group = Some(Pro6Group {
                            name: attr(&e, b"name").unwrap_or_default(),
                            uuid: attr(&e, b"uuid").unwrap_or_default(),
                            slides: Vec::new(),
                        });
                        Scope::Grouping
                    }
                    b"RVDisplaySlide" => {
                        cur_slide = Some(Vec::new());
                        Scope::Slide
                    }
                    b"RVSongArrangement" => {
                        cur_arr = Some(Pro6Arrangement {
                            uuid: attr(&e, b"uuid").unwrap_or_default(),
                            group_ids: Vec::new(),
                        });
                        Scope::Arrangement
                    }
                    b"NSString" => {
                        if attr(&e, b"rvXMLIvarName").as_deref() == Some("RTFData") {
                            text_buf.clear();
                            Scope::RtfData
                        } else if top == Scope::GroupIdsArray {
                            text_buf.clear();
                            Scope::GroupId
                        } else {
                            Scope::Other
                        }
                    }
                    _ => Scope::Other,
                };
                stack.push(scope);
            }
            Ok(Event::Text(t)) => match stack.last().copied().unwrap_or(Scope::Other) {
                Scope::RtfData | Scope::GroupId => {
                    if let Ok(txt) = t.decode() {
                        text_buf.push_str(&txt);
                    }
                }
                _ => {}
            },
            Ok(Event::End(_)) => match stack.pop() {
                Some(Scope::Grouping) => {
                    if let Some(g) = cur_group.take() {
                        groups.push(g);
                    }
                }
                Some(Scope::Slide) => {
                    if let (Some(texts), Some(g)) = (cur_slide.take(), cur_group.as_mut()) {
                        // A slide may hold several text elements; join them.
                        g.slides.push(texts.join("\n"));
                    }
                }
                Some(Scope::Arrangement) => {
                    if let Some(a) = cur_arr.take() {
                        arrangements.push(a);
                    }
                }
                Some(Scope::RtfData) => {
                    if let Some(text) = rtf::rtf_base64_to_text(&text_buf) {
                        if !text.trim().is_empty() {
                            if let Some(s) = cur_slide.as_mut() {
                                s.push(text);
                            }
                        }
                    }
                    text_buf.clear();
                }
                Some(Scope::GroupId) => {
                    let id = text_buf.trim();
                    if !id.is_empty() {
                        if let Some(a) = cur_arr.as_mut() {
                            a.group_ids.push(id.to_string());
                        }
                    }
                    text_buf.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            // Malformed XML: keep what completed cleanly, stop reading.
            Err(_) => break,
            _ => {}
        }
    }

    Pro6Document {
        title,
        selected_arrangement_id: selected,
        groups,
        arrangements,
    }
}

// ── Model → FormattedSong ────────────────────────────────────────────────────────

/// Resolve the group play order: the selected arrangement's group sequence when
/// it names a known arrangement with resolvable groups, else document order.
fn resolve_order(doc: &Pro6Document) -> Vec<&Pro6Group> {
    if !doc.selected_arrangement_id.trim().is_empty() {
        if let Some(arr) = doc
            .arrangements
            .iter()
            .find(|a| a.uuid == doc.selected_arrangement_id)
        {
            let by_uuid: HashMap<&str, &Pro6Group> =
                doc.groups.iter().map(|g| (g.uuid.as_str(), g)).collect();
            let ordered: Vec<&Pro6Group> = arr
                .group_ids
                .iter()
                .filter_map(|id| by_uuid.get(id.as_str()).copied())
                .collect();
            if !ordered.is_empty() {
                return ordered;
            }
        }
    }
    doc.groups.iter().collect()
}

/// Turn the document into a [`FormattedSong`] via the shared assembly:
///
///   * a **named** group → one block labelled by its name (its slides joined, so
///     a multi-page "Chorus" stays one section);
///   * an **unnamed** group → one block per slide (each an auto-numbered verse,
///     which is how simple ProPresenter songs dump every stanza as a slide).
///
/// `finalize` then canonicalizes labels (`normalize_label`), dedups identical
/// blocks (a repeated chorus becomes one section referenced twice) and detects
/// the language.
fn document_to_song(doc: &Pro6Document) -> FormattedSong {
    let mut blocks: Vec<Block> = Vec::new();
    let mut raw_text = String::new();

    for group in resolve_order(doc) {
        let name = group.name.trim();
        if name.is_empty() {
            for slide in &group.slides {
                if slide.trim().is_empty() {
                    continue;
                }
                raw_text.push_str(slide);
                raw_text.push('\n');
                blocks.push((None, slide.lines().map(str::to_string).collect()));
            }
        } else {
            let joined = group
                .slides
                .iter()
                .filter(|s| !s.trim().is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if joined.trim().is_empty() {
                continue;
            }
            raw_text.push_str(&joined);
            raw_text.push('\n');
            blocks.push((
                Some(name.to_string()),
                joined.lines().map(str::to_string).collect(),
            ));
        }
    }

    finalize(doc.title.clone(), &raw_text, blocks)
}

// ── CCLI metadata helpers ────────────────────────────────────────────────────────

/// Trim, returning `None` for empty/whitespace-only.
fn clean(s: Option<&str>) -> Option<String> {
    let t = s?.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Compose a copyright notice from the CCLI author / year / publisher
/// attributes. Author (when present) gets its own line; year and publisher form
/// a natural `© {year} {publisher}` line. Returns `None` when all are empty.
fn compose_copyright(
    author: Option<&str>,
    year: Option<&str>,
    publisher: Option<&str>,
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(a) = clean(author) {
        lines.push(a);
    }
    let copyright_line = match (clean(year), clean(publisher)) {
        (Some(y), Some(p)) => Some(format!("© {y} {p}")),
        (Some(y), None) => Some(format!("© {y}")),
        (None, Some(p)) => Some(p),
        (None, None) => None,
    };
    if let Some(c) = copyright_line {
        lines.push(c);
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The CCLI song id from the dedicated `CCLISongNumber` attribute: its digit run
/// (so `"CCLI 1234"` and `"1234"` both yield `"1234"`), or `None` when empty /
/// digit-free.
fn ccli_song_id(number: Option<&str>) -> Option<String> {
    let raw = clean(number)?;
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    (!digits.is_empty()).then_some(digits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::song_import::{detect_format, ImportFormat};

    // ── fixture builders ─────────────────────────────────────────────────────

    /// Standard-alphabet base64 encoder (B1 only decodes), so tests can embed
    /// readable RTF and encode it the way a real `.pro6` stores `RTFData`.
    fn b64(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0];
            let b1 = *chunk.get(1).unwrap_or(&0);
            let b2 = *chunk.get(2).unwrap_or(&0);
            out.push(T[(b0 >> 2) as usize] as char);
            out.push(T[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(if chunk.len() > 1 {
                T[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                T[(b2 & 0x3f) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    /// Norwegian letters → the `\'xx` CP1252 hex escapes real `.pro6` RTF uses.
    fn escape_no(s: &str) -> String {
        s.replace('æ', r"\'e6")
            .replace('ø', r"\'f8")
            .replace('å', r"\'e5")
            .replace('Æ', r"\'c6")
            .replace('Ø', r"\'d8")
            .replace('Å', r"\'c5")
    }

    /// Wrap lyric lines in a minimal cocoa-style RTF like ProPresenter emits — a
    /// font/colour/expanded-colour table (all must be stripped) then the lines,
    /// `\par`-separated — and base64-encode it as `RTFData` carries it.
    fn rtf_b64(lines: &[&str]) -> String {
        let body = lines
            .iter()
            .map(|l| escape_no(l))
            .collect::<Vec<_>>()
            .join(r"\par ");
        let rtf = format!(
            r"{{\rtf1\ansi\ansicpg1252\cocoartf1504{{\fonttbl\f0\fswiss\fcharset0 Helvetica;}}\
{{\colortbl;\red255\green255\blue255;}}{{\*\expandedcolortbl;;}}\pard\qc\partightenfactor0 \
\f0\fs96 \cf1 {body}}}"
        );
        b64(rtf.as_bytes())
    }

    /// One `<RVDisplaySlide>` carrying the given lyric lines as base64 RTF.
    fn slide_xml(lines: &[&str]) -> String {
        format!(
            "<RVDisplaySlide UUID=\"s\"><array rvXMLIvarName=\"cues\"></array>\
             <array rvXMLIvarName=\"displayElements\"><RVTextElement>\
             <RVRect3D rvXMLIvarName=\"position\">{{0 0 0 100 100}}</RVRect3D>\
             <NSString rvXMLIvarName=\"RTFData\">{}</NSString>\
             </RVTextElement></array></RVDisplaySlide>",
            rtf_b64(lines)
        )
    }

    /// One `<RVSlideGrouping>` with the given name/uuid and slides.
    fn group_xml(name: &str, uuid: &str, slides: &[&[&str]]) -> String {
        let inner: String = slides.iter().map(|s| slide_xml(s)).collect();
        format!(
            "<RVSlideGrouping color=\"0 0 0 0\" name=\"{name}\" uuid=\"{uuid}\">\
             <array rvXMLIvarName=\"slides\">{inner}</array></RVSlideGrouping>"
        )
    }

    /// One `<RVSongArrangement>` referencing group UUIDs in order.
    fn arrangement_xml(uuid: &str, group_ids: &[&str]) -> String {
        let ids: String = group_ids
            .iter()
            .map(|g| format!("<NSString>{g}</NSString>"))
            .collect();
        format!(
            "<RVSongArrangement color=\"0 0 0 0\" name=\"Arr\" uuid=\"{uuid}\">\
             <array rvXMLIvarName=\"groupIDs\">{ids}</array></RVSongArrangement>"
        )
    }

    /// Assemble a whole `.pro6` document from root attributes, groups and
    /// arrangements.
    fn doc_xml(root_attrs: &str, groups: &str, arrangements: &str) -> String {
        format!(
            "<RVPresentationDocument {root_attrs} height=\"1080\" width=\"1920\" versionNumber=\"600\">\
             <RVTimeline rvXMLIvarName=\"timeline\"><array rvXMLIvarName=\"timeCues\"></array></RVTimeline>\
             <array rvXMLIvarName=\"groups\">{groups}</array>\
             <array rvXMLIvarName=\"arrangements\">{arrangements}</array>\
             </RVPresentationDocument>"
        )
    }

    fn labels(song: &FormattedSong) -> Vec<&str> {
        song.sections.iter().map(|s| s.label.as_str()).collect()
    }

    // ── base64-RTF slide text incl. æøå ──────────────────────────────────────

    #[test]
    fn decodes_base64_rtf_with_norwegian_letters() {
        // One unnamed group, one slide: three PD hymn lines with æ/ø/å.
        let groups = group_xml(
            "",
            "G1",
            &[&[
                "Deilig er jorden",
                "prektig er Guds himmel",
                "skjønn er sjelenes pilgrimsgang",
            ]],
        );
        let xml = doc_xml("CCLISongTitle=\"Deilig er jorden\"", &groups, "");
        let song = parse_pro6(&xml);

        assert_eq!(song.title_suggestion.as_deref(), Some("Deilig er jorden"));
        assert_eq!(song.language, "no");
        assert_eq!(song.sections.len(), 1);
        assert_eq!(
            song.sections[0].lyrics,
            "Deilig er jorden\nprektig er Guds himmel\nskjønn er sjelenes pilgrimsgang"
        );
        // No RTF control text (font/colour tables) leaked into the lyrics.
        assert!(!song.sections[0].lyrics.contains('\\'));
        assert!(!song.sections[0].lyrics.contains("Helvetica"));
        assert!(!song.sections[0].lyrics.contains(';'));
    }

    // ── one unnamed group, many slides → one verse per slide ─────────────────

    #[test]
    fn unnamed_group_makes_one_verse_per_slide() {
        // The dominant real-file shape: a single unnamed group whose slides are
        // separate stanzas. Each slide must become its own verse.
        let groups = group_xml(
            "",
            "G1",
            &[
                &["Første vers linje", "andre linje"],
                &["Andre vers linje", "fjerde linje"],
                &["Tredje vers linje"],
            ],
        );
        let xml = doc_xml("CCLISongTitle=\"Tre vers\"", &groups, "");
        let song = parse_pro6(&xml);

        assert_eq!(labels(&song), vec!["verse_1", "verse_2", "verse_3"]);
        assert_eq!(song.sections[0].lyrics, "Første vers linje\nandre linje");
        assert_eq!(song.arrangement, vec!["verse_1", "verse_2", "verse_3"]);
    }

    // ── named groups + grouping → canonical labels; multi-slide joins ─────────

    #[test]
    fn named_groups_map_to_canonical_labels_and_join_slides() {
        let groups = format!(
            "{}{}{}",
            group_xml(
                "Verse 1",
                "GV1",
                &[&["Linje en", "linje to"], &["linje tre"]]
            ),
            group_xml("Refreng", "GC", &[&["Halleluja"]]),
            group_xml("Bridge", "GB", &[&["En bro her"]]),
        );
        let xml = doc_xml("CCLISongTitle=\"Med seksjoner\"", &groups, "");
        let song = parse_pro6(&xml);

        assert_eq!(labels(&song), vec!["verse_1", "chorus", "bridge"]);
        // A named group's two slides joined into one section.
        assert_eq!(song.sections[0].lyrics, "Linje en\nlinje to\nlinje tre");
        assert_eq!(song.sections[1].lyrics, "Halleluja");
    }

    // ── selected arrangement reorders + repeats groups ───────────────────────

    #[test]
    fn selected_arrangement_orders_and_repeats_groups() {
        let groups = format!(
            "{}{}",
            group_xml("Verse 1", "GV1", &[&["Vers ord"]]),
            group_xml("Chorus", "GC", &[&["Refreng ord"]]),
        );
        // Play order: verse, chorus, verse (chorus once as a section).
        let arr = arrangement_xml("ARR1", &["GV1", "GC", "GV1"]);
        let xml = doc_xml(
            "CCLISongTitle=\"Arr\" selectedArrangementID=\"ARR1\"",
            &groups,
            &arr,
        );
        let song = parse_pro6(&xml);

        assert_eq!(labels(&song), vec!["verse_1", "chorus"]);
        assert_eq!(song.arrangement, vec!["verse_1", "chorus", "verse_1"]);
    }

    #[test]
    fn unselected_arrangement_falls_back_to_document_order() {
        // An arrangement exists but is not selected → document order wins (the
        // real-file default: selectedArrangementID empty).
        let groups = format!(
            "{}{}",
            group_xml("Chorus", "GC", &[&["Refreng"]]),
            group_xml("Verse 1", "GV1", &[&["Vers"]]),
        );
        let arr = arrangement_xml("ARR1", &["GV1", "GC"]);
        let xml = doc_xml("CCLISongTitle=\"T\"", &groups, &arr);
        let song = parse_pro6(&xml);
        // Document order: chorus first, then verse — NOT the arrangement order.
        assert_eq!(song.arrangement, vec!["chorus", "verse_1"]);
    }

    #[test]
    fn repeated_identical_slides_dedupe_into_one_section() {
        // Slide 3 repeats slide 1 verbatim (a common "reprise" export).
        let groups = group_xml(
            "",
            "G1",
            &[
                &["Samme linje", "her"],
                &["Ulik linje"],
                &["Samme linje", "her"],
            ],
        );
        let xml = doc_xml("CCLISongTitle=\"T\"", &groups, "");
        let song = parse_pro6(&xml);
        // Two unique sections, arrangement references the first one again.
        assert_eq!(song.sections.len(), 2);
        assert_eq!(song.arrangement, vec!["verse_1", "verse_2", "verse_1"]);
    }

    // ── empty / missing RTFData ──────────────────────────────────────────────

    #[test]
    fn empty_and_missing_rtfdata_slides_are_dropped() {
        // Group with: a real slide, a slide with empty RTFData, and a slide with
        // no RVTextElement at all — only the real one survives.
        let empty_rtf_slide =
            "<RVDisplaySlide><array rvXMLIvarName=\"displayElements\"><RVTextElement>\
             <NSString rvXMLIvarName=\"RTFData\"></NSString></RVTextElement></array></RVDisplaySlide>";
        let no_text_slide =
            "<RVDisplaySlide><array rvXMLIvarName=\"displayElements\"></array></RVDisplaySlide>";
        let groups = format!(
            "<RVSlideGrouping name=\"\" uuid=\"G1\"><array rvXMLIvarName=\"slides\">{}{}{}</array></RVSlideGrouping>",
            slide_xml(&["Ekte ord"]),
            empty_rtf_slide,
            no_text_slide,
        );
        let xml = doc_xml("CCLISongTitle=\"T\"", &groups, "");
        let song = parse_pro6(&xml);
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].lyrics, "Ekte ord");
    }

    #[test]
    fn document_with_no_slides_is_a_warned_stub() {
        let xml = doc_xml("CCLISongTitle=\"Tom\"", "", "");
        let song = parse_pro6(&xml);
        assert_eq!(song.title_suggestion.as_deref(), Some("Tom"));
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    // ── filename-title fallback (empty CCLISongTitle → None) ──────────────────

    #[test]
    fn empty_ccli_title_yields_no_title_suggestion() {
        // Real owner files usually have an empty CCLISongTitle; parse must return
        // None so import_song_file falls back to the filename.
        let groups = group_xml("", "G1", &[&["Noen ord her"]]);
        let xml = doc_xml("CCLISongTitle=\"\"", &groups, "");
        let song = parse_pro6(&xml);
        assert_eq!(song.title_suggestion, None);
        assert_eq!(song.sections.len(), 1);
    }

    // ── malformed XML → graceful degradation ─────────────────────────────────

    #[test]
    fn malformed_xml_degrades_to_a_warned_stub() {
        // Truncated mid-slide: RVDisplaySlide / RVSlideGrouping never close, so
        // nothing is committed — a warned stub, never a panic.
        let broken = "<RVPresentationDocument CCLISongTitle=\"Broken\">\
                      <array rvXMLIvarName=\"groups\"><RVSlideGrouping name=\"\" uuid=\"G1\">\
                      <array rvXMLIvarName=\"slides\"><RVDisplaySlide>\
                      <array rvXMLIvarName=\"displayElements\"><RVTextElement>\
                      <NSString rvXMLIvarName=\"RTFData\">not-closed";
        let song = parse_pro6(broken);
        // Title was read from the root before the break.
        assert_eq!(song.title_suggestion.as_deref(), Some("Broken"));
        assert!(song.sections.is_empty());
        assert!(!song.warnings.is_empty());
    }

    #[test]
    fn total_garbage_never_panics() {
        for src in [
            "",
            "<",
            "<RVPresentationDocument",
            "<RVPresentationDocument>",
            "not xml at all { } [ ]",
            "<RVPresentationDocument><array rvXMLIvarName=\"groups\"><<<>>>",
            "<a><b><c>\u{0}\u{1}</c>",
        ] {
            let _ = parse_pro6(src);
            let _ = extract_metadata(src);
        }
    }

    // ── CCLI metadata (extract_metadata) ─────────────────────────────────────

    #[test]
    fn extract_metadata_reads_ccli_attributes() {
        let xml = doc_xml(
            "CCLISongTitle=\"No Other Name\" CCLIAuthor=\"Jonas Myrin\" \
             CCLICopyrightYear=\"2014\" CCLIPublisher=\"Hillsong Music\" \
             CCLISongNumber=\"7040585\"",
            &group_xml("", "G1", &[&["Line"]]),
            "",
        );
        let meta = extract_metadata(&xml);
        assert_eq!(meta.ccli_song_id.as_deref(), Some("7040585"));
        assert_eq!(
            meta.copyright_notice.as_deref(),
            Some("Jonas Myrin\n© 2014 Hillsong Music")
        );
    }

    #[test]
    fn extract_metadata_empty_ccli_is_all_none() {
        let xml = doc_xml("CCLISongTitle=\"\"", &group_xml("", "G1", &[&["x"]]), "");
        assert_eq!(extract_metadata(&xml), ImportMetadata::default());
    }

    #[test]
    fn compose_copyright_combines_year_and_publisher() {
        assert_eq!(
            compose_copyright(Some("Lina Sandell"), Some("1865"), Some("Public Domain")),
            Some("Lina Sandell\n© 1865 Public Domain".to_string())
        );
        assert_eq!(
            compose_copyright(None, Some("2014"), None),
            Some("© 2014".to_string())
        );
        assert_eq!(
            compose_copyright(None, None, Some("Hillsong")),
            Some("Hillsong".to_string())
        );
        assert_eq!(compose_copyright(Some("  "), Some(""), None), None);
        assert_eq!(compose_copyright(None, None, None), None);
    }

    #[test]
    fn ccli_song_id_extracts_digits() {
        assert_eq!(ccli_song_id(Some("7040585")), Some("7040585".into()));
        assert_eq!(ccli_song_id(Some("CCLI 1234")), Some("1234".into()));
        assert_eq!(ccli_song_id(Some("  ")), None);
        assert_eq!(ccli_song_id(Some("N/A")), None);
        assert_eq!(ccli_song_id(None), None);
    }

    // ── detection lives in song_import but is exercised here ──────────────────

    #[test]
    fn detects_pro6_by_content_and_extension() {
        assert_eq!(
            detect_format("song.pro6", "<RVPresentationDocument CCLISongTitle=\"x\">"),
            ImportFormat::Pro6
        );
        // Content signature wins even with a misleading extension.
        assert_eq!(
            detect_format("song.txt", "<RVPresentationDocument foo=\"bar\">"),
            ImportFormat::Pro6
        );
        // Extension alone is enough.
        assert_eq!(detect_format("Deilig.pro6", "anything"), ImportFormat::Pro6);
        // The binary `.pro` is NOT claimed as .pro6.
        assert_ne!(detect_format("old.pro", "binary"), ImportFormat::Pro6);
    }

    // ── property: never panic, output always internally consistent ────────────

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    const FRAGS: &[&str] = &[
        "<RVPresentationDocument ",
        ">",
        "CCLISongTitle=\"T\"",
        "selectedArrangementID=\"A\"",
        "<array rvXMLIvarName=\"groups\">",
        "<array rvXMLIvarName=\"slides\">",
        "<array rvXMLIvarName=\"groupIDs\">",
        "</array>",
        "<RVSlideGrouping name=\"Verse 1\" uuid=\"G1\">",
        "<RVSlideGrouping name=\"\" uuid=\"G2\">",
        "</RVSlideGrouping>",
        "<RVDisplaySlide>",
        "</RVDisplaySlide>",
        "<RVTextElement>",
        "</RVTextElement>",
        "<NSString rvXMLIvarName=\"RTFData\">",
        "<NSString>",
        "</NSString>",
        "<RVSongArrangement uuid=\"A\">",
        "</RVSongArrangement>",
        "e1xydGYxIGF9",
        "G1",
        "æ",
        "ø",
        "<",
        ">",
        "\"",
        "&amp;",
        "\u{0}",
    ];

    fn random_xml(rng: &mut Lcg) -> String {
        let n = rng.below(40);
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(FRAGS[rng.below(FRAGS.len())]);
        }
        s
    }

    #[test]
    fn fuzz_never_panics_and_is_consistent() {
        use std::collections::HashSet;
        let mut rng = Lcg(0x50_60_70_80_90_a0_b0_c0);
        for _ in 0..500 {
            let xml = random_xml(&mut rng);
            let song = parse_pro6(&xml);
            let _ = extract_metadata(&xml);
            let set: HashSet<&str> = song.sections.iter().map(|s| s.label.as_str()).collect();
            // Every arrangement entry references an existing section.
            for a in &song.arrangement {
                assert!(set.contains(a.as_str()), "dangling ref {a:?} for {xml:?}");
            }
            // Labels are unique; no section is empty.
            assert_eq!(set.len(), song.sections.len(), "dup labels for {xml:?}");
            for s in &song.sections {
                assert!(!s.lyrics.trim().is_empty(), "empty section for {xml:?}");
            }
        }
    }
}
