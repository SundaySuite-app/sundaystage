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

## Rust crates

`sha2` and `hex` (both MIT/Apache-2.0, already present transitively) are used to
checksum-verify each corpus download. All other dependencies are declared in
`src-tauri/Cargo.toml`.
