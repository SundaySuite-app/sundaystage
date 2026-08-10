# Third-party sources adopted by SundayStage

The «Stå på andres skuldre» program allows adopting outside work only under
permissive licenses (MIT / Apache-2.0 / BSD / MPL), verified in the actual
LICENSE file, with attribution recorded here at first adoption.

## Bible corpora — `scrollmapper/bible_databases`

- **Used by:** the full Bible corpus downloader (Spor C, C1/C2) —
  `src-tauri/src/services/bible_download.rs`.
- **Repository:** https://github.com/scrollmapper/bible_databases
- **Pinned ref:** `e1b254cef86d0e65b1a5d1a94b8b112d0f296a2c`
  (`raw.githubusercontent.com/scrollmapper/bible_databases/<ref>/formats/json/<VERSION>.json`).
  A commit ref is content-addressed and immutable; every download is verified
  against a SHA-256 pinned in `CATALOG` before it is parsed or stored.
- **Repository code license:** MIT — "Copyright (c) 2024 Scrollmapper"
  (verified in the repo's `LICENSE` at the pinned ref). The repository _code_
  is what we adopt; the scripture texts themselves are public domain.

### Texts seeded (public domain)

| Code   | File          | Text                                           | Public-domain basis                                      |
| ------ | ------------- | ---------------------------------------------- | -------------------------------------------------------- |
| KJV    | `KJV.json`    | King James Version (1769)                      | Public domain (US); plain verse text, no GPL annotations |
| ASV    | `ASV.json`    | American Standard Version (1901)               | Public domain                                            |
| NB1930 | `Norsk.json`  | Bibelen 1930 (Norwegian bokmål)                | Public domain — confirmed via bibel.no + SWORD `.conf`   |
| NorSMB | `NorSMB.json` | Studentmållagsbibelen 1921 (Norwegian nynorsk) | Public domain                                            |

⚠️ **Not adopted:** CrossWire's _annotated_ KJV (GPL). The `KJV.json` file above
carries plain verse text (Strongs/morphology annotations are not in the `text`
field), so it is the public-domain 1769 text, not the GPL annotated edition.
Underlying-text public domain does **not** imply every edition is public domain —
only the specific files listed here were verified.

## Song interchange formats — Praisenter (format reference)

- **Used by:** the CCLI SongSelect importer (Spor B5) —
  `src-tauri/src/services/import_songselect.rs` — and the OpenLyrics + ChordPro
  EXPORT writers (the lock-in fix) — `src-tauri/src/services/song_export.rs`.
- **Repository:** https://github.com/praisenter/praisenter
- **License:** BSD-3-Clause (verified in the repository's `LICENSE`).
- **What was adopted:** the _format knowledge_ only. Praisenter has reference
  implementations for these documented, observable formats
  (`SongSelectSongFormatProvider` for CCLI `.usr`/`.txt`,
  `OpenLyricsSongFormatProvider`, `ChordProSongFormatProvider`). The Rust here
  was reimplemented idiomatically from the format structure — no Java was copied
  verbatim. The `.usr`/`.txt`, OpenLyrics 0.9 and ChordPro layouts are public,
  documented interchange formats; Praisenter is credited as the reference that
  confirmed their shapes and round-trip semantics.

## Rust crates

`sha2` and `hex` (both MIT/Apache-2.0, already present transitively) are used to
checksum-verify each corpus download.

`quick-xml` (**MIT**, verified in the crate's own `Cargo.toml`, v0.39) parses the
ProPresenter 6 `.pro6` XML for the song importer (Spor B4) —
`src-tauri/src/services/import_propresenter.rs`. It is already present
transitively (pulled by `plist` via the Tauri macOS bundler), so it adds no new
compiled crate; it is declared direct only to depend on it explicitly.

All other dependencies are declared in `src-tauri/Cargo.toml`.
