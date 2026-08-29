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

## Hard-crash capture — Embark Studios `crash-handling`

- **Used by:** the hard-crash signal source (Spor A6) —
  `src-tauri/src/telemetry/native_crash.rs` and its three platform halves.
- **Repository:** https://github.com/EmbarkStudios/crash-handling
- **Crates adopted:** `crash-handler` 0.8 and `crash-context` 0.8.
- **Licenses, verified in the crates' own files:**
  - `crash-handler` — `license = "MIT OR Apache-2.0"` in its `Cargo.toml`, with
    both `LICENSE-MIT` ("Copyright (c) 2019 Embark Studios") and
    `LICENSE-APACHE` shipped in the published crate.
  - `crash-context` — `license = "MIT"` in its `Cargo.toml`, with `LICENSE-MIT`
    and `LICENSE-APACHE` shipped in the published crate.
- **What is adopted:** the handler INSTALLATION and, above all, the
  **chaining**: saving the previous `sigaction` / Mach exception ports /
  unhandled-exception filter and handing the crash back to them when our
  callback returns `Handled(false)`. Getting that wrong is how a home-made crash
  handler makes crashes worse — it swallows them, and the OS's own crash
  reporting, Rust's stack-overflow message and the true exit status all
  disappear with it.
- **What is deliberately NOT adopted:** the rest of that project —
  `minidump-writer`, `minidumper`, `sadness-generator`. **SundayStage writes no
  minidump, on any platform, ever.** A minidump is a byte image of process
  memory, and this process's memory holds the lyrics that were on the
  congregation's screen. See the module docs in `native_crash.rs` and §1 of
  `PRIVACY.md`.

`mach2` 0.6 (**"BSD-2-Clause OR MIT OR Apache-2.0"** in its own `Cargo.toml`,
with `LICENSE-BSD`, `LICENSE-MIT` and `LICENSE-APACHE` shipped) is used on macOS
only, to read the crashed thread's program counter with `thread_get_state` — the
Mach exception message carries no register state. It is pulled in by
`crash-handler` on that platform in any case.

`windows-sys` 0.61 (**MIT OR Apache-2.0**, Microsoft) is declared direct for the
Windows half, for `GetModuleHandleW` / `GetModuleInformation` /
`GetCurrentThreadId`. It is already in the tree many times over transitively, so
declaring it adds no new compiled crate.

All other dependencies are declared in `src-tauri/Cargo.toml`.
