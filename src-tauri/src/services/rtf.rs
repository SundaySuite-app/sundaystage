//! Shoulders program, Spor B1 — shared RTF → plain-text decoder.
//!
//! A hand-rolled decoder for the *subset* of RTF that the worship apps we import
//! from actually emit. It is deliberately a NEW sibling of `song_import`, not a
//! change to any existing parser: B2 (EasyWorship), B4 (ProPresenter `.pro6`) and
//! B6 (`.pro7`) will each call [`rtf_to_text`] on the lyric text they carry.
//! Nothing in the suite calls it yet.
//!
//! Verified against the owner's real files this session:
//!   * **EasyWorship** stores each song's lyrics in a `words rtf` SQLite column
//!     that begins `{\rtf1\ansi\deff0…}`. Lines are `…\plain\f1\fntnamaut <text>\par`;
//!     the header carries `{\fonttbl…}` / `{\colortbl…}` / `{\*\pnseclvl…}` groups
//!     whose literal text (`Arial;`, the `;` colour separators) must NOT leak.
//!   * **ProPresenter `.pro6`** stores each slide's text as base64 inside
//!     `<NSString rvXMLIvarName="RTFData">…</NSString>`; decoded it is
//!     `{\rtf1\ansi\ansicpg1252…}` with Norwegian letters written as `\'e5` (å),
//!     `\'f8` (ø), `\'e6` (æ) hex escapes. `.pro7` embeds the same RTF via protobuf.
//!
//! ## Why hand-rolled and not a crate
//!
//! The most popular candidate, `rtf-parser` (v0.4.3, MIT, ~149k downloads), is by
//! its own description "a Rust RTF parser & lexer library" — it builds a token/AST
//! stream and does **not** advertise plain-text extraction. Adopting it would add
//! a dependency (plus its transitive tree) and we would *still* have to write the
//! tree-walk that produces the semantics we actually need: control-word → newline
//! mapping, `\'xx` decoding through the Windows-1252 ANSI codepage, `\uN?` with
//! fallback consumption, and `{\*\…}` / font/colour-table destination stripping.
//! That semantic layer — the hard part — is ours either way, so the crate buys
//! little over the ~200 lines below, and hand-rolling matches the house style of
//! `song_import` (pure, dependency-free, fixture-tested). If the subset ever needs
//! to grow toward full RTF, revisit `rtf-parser`.
//!
//! Everything here is pure and never panics: malformed input degrades to a
//! best-effort string (or an empty one), never a crash — the live path is holy.

/// Decode an RTF document (the `words` column value, or a decoded `RTFData`
/// blob as a string) into plain text with `\n` line breaks.
///
/// Control words `\par` / `\line` / `\sect` and a trailing `\`-newline become
/// newlines; `\tab` becomes a tab; `\'xx` decodes through Windows-1252 (so
/// `\'e6\'f8\'e5` → `æøå`); `\uN?` becomes the Unicode scalar and its fallback
/// glyphs are consumed; `{\fonttbl…}`, `{\colortbl…}`, `{\stylesheet…}` and any
/// `{\*\…}` destination are dropped whole; unknown control words are ignored.
/// Leading/trailing blank lines are trimmed but interior blank lines (stanza
/// breaks) are preserved.
pub fn rtf_to_text(rtf: &str) -> String {
    let chars: Vec<char> = rtf.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    let mut out = String::new();

    // One frame per open `{…}` group; the base frame is the document itself.
    // `ignore` suppresses text output (destinations / font & colour tables);
    // `uc` is the `\ucN` Unicode fallback-skip count, inherited by child groups.
    let mut stack: Vec<Frame> = vec![Frame {
        ignore: false,
        uc: 1,
    }];

    while i < n {
        let ignore = stack.last().map(|f| f.ignore).unwrap_or(false);
        let c = chars[i];
        match c {
            '{' => {
                let parent = stack.last().copied().unwrap_or(Frame {
                    ignore: false,
                    uc: 1,
                });
                stack.push(parent);
                i += 1;
            }
            '}' => {
                if stack.len() > 1 {
                    stack.pop();
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= n {
                    break;
                }
                let d = chars[i];
                if d.is_ascii_alphabetic() {
                    // Control word: name = maximal ASCII-letter run.
                    let start = i;
                    while i < n && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let name: String = chars[start..i].iter().collect();
                    // Optional signed numeric parameter.
                    let neg = i < n && chars[i] == '-';
                    if neg {
                        i += 1;
                    }
                    let pstart = i;
                    while i < n && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    let param: Option<i64> = if i > pstart {
                        chars[pstart..i]
                            .iter()
                            .collect::<String>()
                            .parse::<i64>()
                            .ok()
                            .map(|v| if neg { -v } else { v })
                    } else {
                        None
                    };
                    // A single space is the control-word delimiter and is consumed.
                    if i < n && chars[i] == ' ' {
                        i += 1;
                    }
                    apply_control_word(&name, param, &mut stack, &mut out, &chars, &mut i, ignore);
                } else {
                    // Control symbol.
                    match d {
                        '\'' => {
                            // `\'xx` — two hex digits, decoded via the ANSI codepage.
                            i += 1;
                            let hi = chars.get(i).and_then(|c| c.to_digit(16));
                            let lo = chars.get(i + 1).and_then(|c| c.to_digit(16));
                            if let (Some(h), Some(l)) = (hi, lo) {
                                i += 2;
                                if !ignore {
                                    out.push(cp1252_byte_to_char((h * 16 + l) as u8));
                                }
                            }
                            // Malformed (`\'` without two hex digits): drop it.
                        }
                        '*' => {
                            // Ignorable-destination marker: whole group is dropped.
                            if let Some(f) = stack.last_mut() {
                                f.ignore = true;
                            }
                            i += 1;
                        }
                        '\\' | '{' | '}' => {
                            if !ignore {
                                out.push(d);
                            }
                            i += 1;
                        }
                        '~' => {
                            if !ignore {
                                out.push(' '); // non-breaking space → plain space
                            }
                            i += 1;
                        }
                        '_' => {
                            if !ignore {
                                out.push('-'); // non-breaking hyphen → plain hyphen
                            }
                            i += 1;
                        }
                        '-' => i += 1, // optional hyphen: nothing when not breaking
                        '\n' | '\r' => {
                            if !ignore {
                                out.push('\n'); // `\`-at-line-end line break
                            }
                            i += 1;
                        }
                        _ => i += 1, // unknown control symbol
                    }
                }
            }
            '\r' | '\n' => i += 1, // raw line breaks in the RTF stream are ignored
            _ => {
                if !ignore {
                    out.push(c);
                }
                i += 1;
            }
        }
    }

    normalize(&out)
}

/// Decode base64 (standard alphabet, whitespace tolerated, `=` padding stops the
/// stream) into raw bytes. Returns `None` on an invalid character.
///
/// ProPresenter carries RTF as base64; this keeps that transport concern out of
/// [`rtf_to_text`], which takes a string. Pair it with [`rtf_bytes_to_text`] (or
/// use [`rtf_base64_to_text`]).
pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn sextet(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &byte in input.as_bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let v = sextet(byte)?;
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// Decode RTF bytes (each interpreted through the Windows-1252 ANSI codepage)
/// into plain text. Use this for a decoded ProPresenter `RTFData` blob.
pub fn rtf_bytes_to_text(bytes: &[u8]) -> String {
    let s: String = bytes.iter().map(|&b| cp1252_byte_to_char(b)).collect();
    rtf_to_text(&s)
}

/// Convenience for the ProPresenter path: base64 → bytes → RTF → plain text.
/// Returns `None` only when the base64 itself is invalid.
pub fn rtf_base64_to_text(b64: &str) -> Option<String> {
    decode_base64(b64).map(|bytes| rtf_bytes_to_text(&bytes))
}

/// State for one open `{…}` group.
#[derive(Clone, Copy)]
struct Frame {
    ignore: bool,
    uc: i32,
}

/// Dispatch a control word. May advance `i` further (Unicode fallback, `\bin`).
fn apply_control_word(
    name: &str,
    param: Option<i64>,
    stack: &mut [Frame],
    out: &mut String,
    chars: &[char],
    i: &mut usize,
    ignore: bool,
) {
    let n = chars.len();
    match name {
        "par" | "line" | "sect" if !ignore => out.push('\n'),
        "tab" if !ignore => out.push('\t'),
        "uc" => {
            if let (Some(f), Some(p)) = (stack.last_mut(), param) {
                f.uc = p.clamp(0, i32::MAX as i64) as i32;
            }
        }
        "u" => {
            if let Some(p) = param {
                // 16-bit signed value; negatives wrap into the BMP.
                let cp = p.rem_euclid(65536) as u32;
                if !ignore {
                    if let Some(ch) = char::from_u32(cp) {
                        out.push(ch);
                    }
                }
            }
            // Skip the `\ucN` fallback representation that follows.
            let uc = stack.last().map(|f| f.uc).unwrap_or(1).max(0);
            let mut skipped = 0;
            while skipped < uc && *i < n {
                match chars[*i] {
                    '{' | '}' => break, // never cross a group boundary
                    '\\' => {
                        *i += 1;
                        if *i < n {
                            if chars[*i] == '\'' {
                                *i += 1;
                                let mut k = 0;
                                while k < 2 && *i < n && chars[*i].is_ascii_hexdigit() {
                                    *i += 1;
                                    k += 1;
                                }
                            } else if chars[*i].is_ascii_alphabetic() {
                                while *i < n && chars[*i].is_ascii_alphabetic() {
                                    *i += 1;
                                }
                                if *i < n && chars[*i] == '-' {
                                    *i += 1;
                                }
                                while *i < n && chars[*i].is_ascii_digit() {
                                    *i += 1;
                                }
                                if *i < n && chars[*i] == ' ' {
                                    *i += 1;
                                }
                            } else {
                                *i += 1;
                            }
                        }
                    }
                    _ => *i += 1,
                }
                skipped += 1;
            }
        }
        // Destinations whose literal text must never leak into lyrics.
        "fonttbl" | "colortbl" | "stylesheet" | "info" | "generator" | "filetbl" | "listtable"
        | "listoverridetable" | "revtbl" | "pntext" | "pntxta" | "pntxtb" | "themedata"
        | "colorschememapping" | "datastore" | "xmlnstbl" => {
            if let Some(f) = stack.last_mut() {
                f.ignore = true;
            }
        }
        // `\binN` — skip N following units so embedded binary never leaks as text.
        "bin" => {
            if let Some(p) = param {
                *i = i.saturating_add(p.max(0) as usize).min(n);
            }
        }
        _ => {} // unknown control word: no output
    }
}

/// Map a byte through the Windows-1252 (ANSI) codepage the worship apps declare
/// (`\ansicpg1252`). 0x00–0x7F and 0xA0–0xFF are Latin-1 (so `æøå` at
/// 0xE6/0xF8/0xE5 are exact); only 0x80–0x9F need the CP1252 punctuation map.
fn cp1252_byte_to_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}', // €
        0x82 => '\u{201A}', // ‚
        0x83 => '\u{0192}', // ƒ
        0x84 => '\u{201E}', // „
        0x85 => '\u{2026}', // …
        0x86 => '\u{2020}', // †
        0x87 => '\u{2021}', // ‡
        0x88 => '\u{02C6}', // ˆ
        0x89 => '\u{2030}', // ‰
        0x8A => '\u{0160}', // Š
        0x8B => '\u{2039}', // ‹
        0x8C => '\u{0152}', // Œ
        0x8E => '\u{017D}', // Ž
        0x91 => '\u{2018}', // ‘
        0x92 => '\u{2019}', // ’
        0x93 => '\u{201C}', // “
        0x94 => '\u{201D}', // ”
        0x95 => '\u{2022}', // •
        0x96 => '\u{2013}', // –
        0x97 => '\u{2014}', // —
        0x98 => '\u{02DC}', // ˜
        0x99 => '\u{2122}', // ™
        0x9A => '\u{0161}', // š
        0x9B => '\u{203A}', // ›
        0x9C => '\u{0153}', // œ
        0x9E => '\u{017E}', // ž
        0x9F => '\u{0178}', // Ÿ
        // Latin-1 range (and the five undefined CP1252 slots) map 1:1.
        other => other as char,
    }
}

/// Trim trailing whitespace per line and drop leading/trailing blank lines,
/// while preserving interior blank lines (stanza breaks).
fn normalize(raw: &str) -> String {
    let lines: Vec<String> = raw.split('\n').map(|l| l.trim_end().to_string()).collect();
    let start = lines.iter().position(|l| !l.is_empty());
    let end = lines.iter().rposition(|l| !l.is_empty());
    match (start, end) {
        (Some(s), Some(e)) => lines[s..=e].join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── control words → line breaks ─────────────────────────────────────────────

    #[test]
    fn par_line_and_sect_become_newlines() {
        assert_eq!(rtf_to_text(r"{\rtf1 a\par b\line c\sect d}"), "a\nb\nc\nd");
    }

    #[test]
    fn par_with_numeric_parameter_still_breaks() {
        // `\parN` (a control word with a numeric arg) is still a paragraph break.
        assert_eq!(rtf_to_text(r"{\rtf1 a\par0 b}"), "a\nb");
    }

    #[test]
    fn tab_becomes_a_tab() {
        assert_eq!(rtf_to_text(r"{\rtf1 a\tab b}"), "a\tb");
    }

    #[test]
    fn raw_stream_newlines_are_ignored() {
        // Literal CR/LF inside the RTF are not paragraph breaks.
        assert_eq!(rtf_to_text("{\\rtf1 one\ntwo\r\nthree}"), "onetwothree");
    }

    // ── group nesting & formatting flattening ───────────────────────────────────

    #[test]
    fn nested_groups_and_formatting_are_flattened() {
        let src = r"{\rtf1 plain {\b bold {\i italic}} tail}";
        assert_eq!(rtf_to_text(src), "plain bold italic tail");
    }

    #[test]
    fn escaped_braces_and_backslash_are_literal() {
        assert_eq!(rtf_to_text(r"{\rtf1 a\{b\}c\\d}"), "a{b}c\\d");
    }

    // ── \'xx hex escapes: Norwegian correctness (hard requirement) ──────────────

    #[test]
    fn hex_escapes_decode_norwegian_lowercase() {
        assert_eq!(rtf_to_text(r"{\rtf1 \'e6\'f8\'e5}"), "æøå");
    }

    #[test]
    fn hex_escapes_decode_norwegian_uppercase() {
        assert_eq!(rtf_to_text(r"{\rtf1 \'c6\'d8\'c5}"), "ÆØÅ");
    }

    #[test]
    fn hex_escape_in_a_word() {
        assert_eq!(rtf_to_text(r"{\rtf1 b\'f8nn}"), "bønn");
    }

    #[test]
    fn cp1252_high_punctuation_is_mapped_not_latin1() {
        // 0x92 is a right single quote in CP1252, not U+0092.
        assert_eq!(rtf_to_text(r"{\rtf1 it\'92s}"), "it\u{2019}s");
    }

    // ── \uNNNN? unicode escapes ─────────────────────────────────────────────────

    #[test]
    fn unicode_escapes_decode_norwegian() {
        // æ=230 ø=248 å=229, each with the single `?` fallback char.
        assert_eq!(rtf_to_text(r"{\rtf1 \u230?\u248?\u229?}"), "æøå");
    }

    #[test]
    fn unicode_negative_value_wraps_into_bmp() {
        // ø = 248; the signed-16-bit spelling is 248 - 65536 = -65288.
        assert_eq!(rtf_to_text(r"{\rtf1 \u-65288?}"), "ø");
    }

    #[test]
    fn unicode_respects_uc_multichar_fallback() {
        // `\uc2` → skip two fallback chars ("[]") after each `\uNNNN`.
        assert_eq!(rtf_to_text(r"{\rtf1 \uc2 \u248[]after}"), "øafter");
    }

    #[test]
    fn unicode_fallback_can_be_a_hex_escape() {
        // Common real spelling: `\uNNNN\'3f` (the `?` written as a hex escape).
        assert_eq!(rtf_to_text(r"{\rtf1 \u248\'3fx}"), "øx");
    }

    // ── destination / table stripping ───────────────────────────────────────────

    #[test]
    fn font_and_color_tables_are_stripped() {
        // These leak `Arial;` / `Verdana;` and the `;` colour separators unless
        // the whole destination group is dropped.
        let src = r"{\rtf1\ansi\deff0{\fonttbl{\f0\fnil\fcharset0 Arial;}{\f1\fnil\fcharset0 Verdana;}}{\colortbl\red0\green0\blue0;\red255\green0\blue0;}Hello}";
        let out = rtf_to_text(src);
        assert_eq!(out, "Hello");
        assert!(!out.contains("Arial"));
        assert!(!out.contains("Verdana"));
        assert!(!out.contains(';'));
    }

    #[test]
    fn star_destinations_are_stripped() {
        let src = r"{\rtf1 {\*\generator Riched20 10.0}{\*\pnseclvl1\pnucrm{\pntxtb}{\pntxta{.}}}Keep me}";
        assert_eq!(rtf_to_text(src), "Keep me");
    }

    #[test]
    fn stylesheet_group_is_stripped() {
        let src = r"{\rtf1{\stylesheet{\s0 Normal;}{\s1 heading 1;}}Body text}";
        assert_eq!(rtf_to_text(src), "Body text");
    }

    // ── realistic multi-stanza fixtures (synthesised, not owner's lyrics) ────────

    #[test]
    fn easyworship_multi_stanza_roundtrip() {
        // Mirrors the real EasyWorship `words` column structure verified this
        // session (header groups + `\plain\f1\fntnamaut <text>\par` lines +
        // a bare `\par` blank line between stanzas). Lyrics are synthetic
        // generic phrases containing æ/ø/å, not any specific copyrighted song.
        let src = r#"{\rtf1\ansi\deff0\deftab254{\fonttbl{\f0\fnil\fcharset0 Arial;}{\f1\fnil\fcharset0 Verdana;}}{\colortbl\red0\green0\blue0;\red255\green0\blue0;}{\*\pnseclvl1\pnucrm\pnstart1\pnhang\pnindent720{\pntxtb}{\pntxta{.}}}
\li0\fi0\ri0\sb0\sl\sa0 \plain\f1\fntnamaut Vers 1\par
\li0\fi0\ri0\sb0\sl\sa0 \plain\f1\fntnamaut Guds n\'e5de er stor\par
\li0\fi0\ri0\sb0\sl\sa0 \plain\f1\fntnamaut Herren h\'f8rer min b\'f8nn\par
\li0\fi0\ri0\sb0\sl\sa0 \par
\li0\fi0\ri0\sb0\sl\sa0 \plain\f1\fntnamaut Vers 2\par
\li0\fi0\ri0\sb0\sl\sa0 \plain\f1\fntnamaut Hans kj\'e6rlighet n\'e5r meg}"#;
        let expected = "Vers 1\n\
                        Guds nåde er stor\n\
                        Herren hører min bønn\n\
                        \n\
                        Vers 2\n\
                        Hans kjærlighet når meg";
        assert_eq!(rtf_to_text(src), expected);
    }

    #[test]
    fn propresenter_slide_with_unicode_and_hex() {
        // ProPresenter mixes `\'xx` and paragraph runs; `\pard` resets and emits
        // nothing (distinct from `\par`).
        let src = r"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\froman Helvetica;}}\pard\f0\fs96 Syng for Herren\par en ny s\'e5ng}";
        assert_eq!(rtf_to_text(src), "Syng for Herren\nen ny sång");
    }

    // ── base64 transport (ProPresenter path) ────────────────────────────────────

    #[test]
    fn decode_base64_known_vectors() {
        assert_eq!(decode_base64("TWFu").unwrap(), b"Man");
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        // Whitespace is tolerated.
        assert_eq!(decode_base64("TW\n Fu").unwrap(), b"Man");
    }

    #[test]
    fn decode_base64_rejects_invalid() {
        assert!(decode_base64("not*valid").is_none());
    }

    #[test]
    fn rtf_base64_to_text_full_pipeline() {
        // base64 of `{\rtf1\ansi\ansicpg1252 Guds n\'e5de er stor}`
        let b64 = "e1xydGYxXGFuc2lcYW5zaWNwZzEyNTIgR3VkcyBuXCdlNWRlIGVyIHN0b3J9";
        assert_eq!(
            rtf_base64_to_text(b64).as_deref(),
            Some("Guds nåde er stor")
        );
    }

    #[test]
    fn rtf_base64_to_text_multiline_norwegian() {
        // base64 of `{\rtf1\ansi Deilig er jorden\par Pr\'e6ktig er Guds himmel}`
        // (public-domain hymn line, used only to prove the \par + \'xx pipeline).
        let b64 =
            "e1xydGYxXGFuc2kgRGVpbGlnIGVyIGpvcmRlblxwYXIgUHJcJ2U2a3RpZyBlciBHdWRzIGhpbW1lbH0=";
        assert_eq!(
            rtf_base64_to_text(b64).as_deref(),
            Some("Deilig er jorden\nPræktig er Guds himmel")
        );
    }

    #[test]
    fn rtf_bytes_to_text_maps_high_bytes_through_cp1252() {
        // A raw 0xF8 byte (not a `\'` escape) is literal ø text via CP1252.
        let bytes = b"{\\rtf1 b\xf8nn}";
        assert_eq!(rtf_bytes_to_text(bytes), "bønn");
    }

    // ── stanza / whitespace handling ────────────────────────────────────────────

    #[test]
    fn interior_blank_lines_preserved_edges_trimmed() {
        let src = r"{\rtf1 \par\par a\par\par b\par\par}";
        // Leading + trailing blanks gone; the interior stanza break survives.
        assert_eq!(rtf_to_text(src), "a\n\nb");
    }

    #[test]
    fn trailing_spaces_per_line_are_trimmed() {
        assert_eq!(
            rtf_to_text(r"{\rtf1 line one   \par line two   }"),
            "line one\nline two"
        );
    }

    // ── empty / malformed: graceful degradation, never a panic ───────────────────

    #[test]
    fn empty_and_malformed_never_panic() {
        for src in [
            "",
            "{",
            "}",
            "{{{{",
            "}}}}",
            r"{\rtf1",
            r"\'",           // hex escape with no digits
            r"{\rtf1 \'zz}", // non-hex after \'
            r"\",            // trailing lone backslash
            r"{\u}",         // \u with no number
            r"{\uc}",        // \uc with no number
            r"{\bin}",       // \bin with no number
            r"{\rtf1 {\*\ unterminated destination",
            "{\\rtf1 \u{0}\u{1}\u{7} raw control bytes}",
        ] {
            let _ = rtf_to_text(src); // must return, not crash
        }
        assert_eq!(rtf_to_text(""), "");
        assert_eq!(rtf_to_text("{}"), "");
        // A huge \uNNNN value (built at runtime to avoid a source `\u` literal)
        // must wrap into the BMP and never panic.
        let _ = rtf_to_text(&format!(r"{{\rtf1 \u{}?}}", i64::MAX));
        let _ = rtf_to_text(&format!(r"{{\rtf1 \u{}?}}", i64::MIN));
    }

    #[test]
    fn plain_text_without_rtf_header_passes_through() {
        // Defensive: a non-RTF string still yields something sane, not a panic.
        assert_eq!(rtf_to_text("just plain text"), "just plain text");
    }

    #[test]
    fn bin_data_is_skipped_not_leaked() {
        // `\bin5` then 5 raw units of "binary" must not appear in the output.
        assert_eq!(rtf_to_text(r"{\rtf1 a\bin5 XYZWQb}"), "ab");
    }
}
