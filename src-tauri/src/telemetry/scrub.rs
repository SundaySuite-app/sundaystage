//! E3 — scrubbing text that is about to be written somewhere it can be read
//! later.
//!
//! Law 2 of the telemetry programme is that **content never leaves the
//! machine**: no lyrics, no song or service titles, no file paths, no device
//! names. Everything in this module exists to make that true of the ONE class
//! of string a developer can accidentally widen — the formatted free text in a
//! panic message, a log line or an error.
//!
//! ## Why the scrubbing happens at WRITE time, not at send time
//!
//! The crash ring and the log file are written to be read later — E5/E6 offer
//! to upload them. A file that is only safe once a sender remembers to clean it
//! is a file that leaks the first time someone attaches it to an email. So the
//! records land on disk already wire-shaped, and [`crate::telemetry::logfile`]
//! scrubs a SECOND time when the tail is read back.
//!
//! ## The half-washed-path bug (SundayRec, 2026-08-08)
//!
//! The lesson this module is built around. SundayRec's first version replaced
//! the home directory with `~` (so `/Users/kari/Opptak/gudstjeneste.wav` became
//! `~/Opptak/gudstjeneste.wav`) and then tokenised on WHITESPACE to decide what
//! looked like a path. Two things went wrong at once:
//!
//!   - `Some("~/Opptak/gudstjeneste.wav")` is one whitespace token that starts
//!     with `S`, so the tokeniser decided it was not a path and let it through;
//!   - the operator's NAME was gone, so nothing looked obviously wrong locally.
//!
//! The endpoint's own `ABSOLUTE_PATH_RE` then rejected the payload with a 400,
//! the client dropped it without retry, and the crash reports vanished silently.
//! The fix — and the shape here — is to scan for path STARTS rather than
//! tokenise, and to consume the WHOLE path run including the filename.
//!
//! [`fixtures`] emits the exact outputs of this module for a canonical input
//! set into `src-tauri/telemetry-scrub-fixtures.json`. E4 feeds those exact
//! strings to the Worker's validator, so the two repositories agree about what
//! "scrubbed" means instead of each believing its own tests.

/// What an absolute path is replaced with. Deliberately not empty: a reader
/// should see that something was removed rather than read a mangled sentence.
pub const PATH_PLACEHOLDER: &str = "<path>";

/// What a user name inside a path root is replaced with.
pub const USER_PLACEHOLDER: &str = "<user>";

/// Path roots whose FIRST segment is a user name.
const USER_ROOTS: &[&str] = &["/users/", "/home/", "\\users\\"];

/// Roots that name an absolute location ON THEIR OWN, matched anywhere in the
/// text — no left boundary required, because they cannot mean anything else.
/// Lowercase; matched against an ASCII-lowercased copy (macOS is
/// case-insensitive and Windows APIs hand back any casing).
const ABSOLUTE_ROOTS: &[&str] = &[
    "/users/",
    "/home/",
    "/private/",
    "/tmp/",
    "/var/",
    "/volumes/",
    "/applications/",
    "/library/",
    "/opt/",
    "/etc/",
    "/usr/",
    "/mnt/",
    "/media/",
    "/root/",
    "/srv/",
    "\\users\\",
];

/// Characters that STRUCTURE a formatted message rather than name a file.
///
/// They do two jobs at once: a path may START right after one
/// (`Some("/Users/…`, `open(/Users/…`) — the case the old whitespace tokeniser
/// missed — and a path ENDS at one (`…x.wav")`), so the closing punctuation and
/// the rest of the sentence survive the replacement.
const PATH_DELIMITERS: &[char] = &[
    '"', '\'', '`', '(', ')', '[', ']', '{', '}', '<', '>', '«', '»', ',', ';', '=', '|',
];

/// Whether `c` can sit INSIDE a path — i.e. neither whitespace nor structure.
///
/// `:` counts, which is load-bearing twice: it keeps `C:\Users\…` a single run,
/// and it means the `/` in `https://host/x` is not preceded by a boundary, so a
/// URL survives intact instead of becoming `https:<path>`. A URL is a statement
/// about a server, not about someone's disk, and it is worth keeping.
fn is_path_body(c: char) -> bool {
    !c.is_whitespace() && !PATH_DELIMITERS.contains(&c)
}

/// Replace absolute user paths in `text` so it carries no operator identity.
///
/// Two passes, in this order:
///
///   1. `home` (when given) becomes `~`, so the common case reads naturally;
///   2. any remaining `/Users/<seg>`, `/home/<seg>` or `\Users\<seg>` has its
///      `<seg>` replaced with [`USER_PLACEHOLDER`] — this catches paths that are
///      NOT under this user's home (a crate built in someone else's checkout).
///
/// This is step one of two. On its own it leaves `~/Musikk/Julekonsert.mp4`,
/// which no longer names the OPERATOR but still names a folder and a service —
/// see [`strip_absolute_paths`].
pub fn scrub_paths(text: &str, home: Option<&str>) -> String {
    let out = match home.map(str::trim).filter(|h| !h.is_empty()) {
        // Strip a trailing separator first so `/Users/ola/` and `/Users/ola`
        // both collapse to the same `~` without leaving a doubled separator.
        Some(home) => {
            let trimmed = home.trim_end_matches(['/', '\\']);
            if trimmed.is_empty() {
                text.to_string()
            } else {
                text.replace(trimmed, "~")
            }
        }
        None => text.to_string(),
    };
    scrub_user_roots(&out)
}

/// The second pass of [`scrub_paths`]: rewrite the first segment after a known
/// user root. Split out so it can be tested on its own.
fn scrub_user_roots(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        let hit = USER_ROOTS
            .iter()
            .filter_map(|root| lower[i..].find(root).map(|at| (i + at, root.len())))
            .min_by_key(|(at, _)| *at);
        let Some((at, root_len)) = hit else {
            out.push_str(&text[i..]);
            break;
        };
        // Everything up to and including the root marker is copied verbatim so
        // the original casing/separator survives.
        out.push_str(&text[i..at + root_len]);
        let rest = &text[at + root_len..];
        let seg_end = rest.find(['/', '\\']).unwrap_or(rest.len());
        if seg_end == 0 {
            // `/Users//x` or a trailing `/Users/` — nothing to redact.
            i = at + root_len;
            continue;
        }
        out.push_str(USER_PLACEHOLDER);
        i = at + root_len + seg_end;
    }
    out
}

/// Whether an absolute path begins at byte offset `i`.
///
/// `prev` is the character immediately to the left (`None` at the start).
/// `lower` is an ASCII-lowercased copy of `text` — lowercasing ASCII never
/// changes a byte's length, so offsets into the two are interchangeable.
///
/// Two tiers: the [`ABSOLUTE_ROOTS`] and `~/` are unmistakable and match
/// anywhere; a bare `/`, a UNC `\\` and a `X:\` drive are ambiguous and match
/// only at a LEFT BOUNDARY. That boundary is what keeps RELATIVE paths intact —
/// `src/output/process.rs:301:9` is this repository's own layout, identical on
/// every machine, and the most useful thing a crash location carries.
fn path_starts_at(text: &str, lower: &str, i: usize, prev: Option<char>) -> bool {
    let rest = &text[i..];
    if ABSOLUTE_ROOTS.iter().any(|r| lower[i..].starts_with(r)) {
        return true;
    }
    // `~/x` is what pass 1 of `scrub_paths` leaves behind. Consuming it here is
    // the whole half-washed-path fix.
    if rest.starts_with("~/") || rest.starts_with("~\\") {
        return true;
    }
    // Anchored shapes below this line.
    if prev.is_some_and(is_path_body) {
        return false;
    }
    let b = rest.as_bytes();
    // `X:\` or `X:/` — a Windows drive. One letter only, so a URL scheme
    // (`https:`) can never be mistaken for one.
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/') {
        return true;
    }
    // `\\NAS\share` — a UNC share. Requires something after the two
    // backslashes, so a lone `\\` in prose is not a path.
    if let Some(after) = rest.strip_prefix("\\\\") {
        return after
            .chars()
            .next()
            .is_some_and(|c| c != '\\' && is_path_body(c));
    }
    // A bare POSIX root. Requires a following body character, so the `/` in
    // "og / eller" stays a slash.
    if let Some(after) = rest.strip_prefix('/') {
        return after.chars().next().is_some_and(is_path_body);
    }
    false
}

/// Byte offset one past the end of the path run starting at `from`.
///
/// Runs to the first whitespace or [`PATH_DELIMITERS`] character — EXCEPT that a
/// literal [`USER_PLACEHOLDER`] is swallowed whole. [`scrub_paths`] has already
/// run and may have spliced `<user>` into the middle of the path; without this
/// the `>` would end the run and leave the rest of the path (the folder and the
/// filename — exactly what we are here to remove) behind.
fn path_run_end(text: &str, from: usize) -> usize {
    let mut j = from;
    while j < text.len() {
        let rest = &text[j..];
        if rest.starts_with(USER_PLACEHOLDER) {
            j += USER_PLACEHOLDER.len();
            continue;
        }
        let Some(c) = rest.chars().next().filter(|c| is_path_body(*c)) else {
            break;
        };
        j += c.len_utf8();
    }
    j
}

/// Replace every absolute path in `text` with [`PATH_PLACEHOLDER`], whether it
/// stands alone between spaces or sits EMBEDDED in a formatted value.
///
/// What survives is the structure around the path — `Some("<path>")`,
/// `open(<path>)` — which is the part that says what the code was doing.
///
/// One residual, named honestly and NOT fixed here. A path run stops at the
/// first whitespace, so a filename containing a space leaves its tail behind:
///
///   `kunne ikke åpne ~/Musikk/Julekonsert 2026.mp4`
///     → `kunne ikke åpne <path> 2026.mp4`
///
/// Every way to close it guesses. A path run cannot simply eat spaces —
/// `kunne ikke åpne /tmp/x fordi disken er full` would swallow the whole
/// sentence, and losing the message loses the only thing that tells two crashes
/// apart. Anything smarter is a heuristic in the one place that must be
/// obviously correct. It is closed where the NAME is instead: the tracing audit
/// (see `crate::telemetry::logfile`'s formatter test) keeps operator-authored
/// titles out of formatted strings in the first place.
///
/// Idempotent: `<path>` holds no separator, drive or `~`, so a second pass finds
/// no start inside it.
pub fn strip_absolute_paths(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    let mut prev: Option<char> = None;
    while i < text.len() {
        if path_starts_at(text, &lower, i, prev) {
            out.push_str(PATH_PLACEHOLDER);
            i = path_run_end(text, i);
            // The placeholder ends in `>`, a delimiter — so anything directly
            // after it is at a boundary, as it would have been after the path.
            prev = Some('>');
            continue;
        }
        // `i` always sits on a character boundary: it is advanced either by a
        // whole `char` here or to a run end, itself found by walking whole
        // `char`s. Norwegian text is never cut mid-character.
        let c = text[i..].chars().next().expect("i is a char boundary");
        out.push(c);
        prev = Some(c);
        i += c.len_utf8();
    }
    out
}

/// The user's home directory, for path scrubbing. `None` in an environment that
/// sets neither variable (a service account, a sandboxed CI runner) — the
/// [`strip_absolute_paths`] pass does not depend on it.
pub fn home_dir() -> Option<String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|v| v.to_string_lossy().into_owned())
}

/// One line on its way into (or out of) the log file: both path passes, no
/// truncation. The log is the one artefact a volunteer will cheerfully paste
/// into a chat channel.
pub fn for_log(text: &str) -> String {
    strip_absolute_paths(&scrub_paths(text, home_dir().as_deref()))
}

/// The only route by which developer-authored free text reaches a persisted
/// record — a panic message, a panic location, a task name.
///
/// Three passes: [`scrub_paths`], then [`strip_absolute_paths`], then a hard
/// character cap. The cap counts CHARS, not bytes, so a Norwegian message is
/// never cut mid-character, and the ellipsis is PUSHED AFTER the truncation, so
/// a capped string is exactly `max + 1` characters. That `+1` convention is
/// shared with the Worker's validator (law 3: every free-text limit gets a test
/// spanning both repositories) — a validator that rejected `max + 1` would
/// silently 400 every long message.
pub fn free_text(raw: &str, home: Option<&str>, max: usize) -> String {
    let cleaned = strip_absolute_paths(&scrub_paths(raw, home));
    truncate_with_ellipsis(&cleaned, max)
}

/// Cut `text` to `max` CHARS and push `…` if anything was removed, giving a
/// result of exactly `max + 1` chars. Never cuts mid-character.
pub fn truncate_with_ellipsis(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// The canonical input set whose scrubbed outputs are shared with the Worker.
///
/// Every entry is a shape that actually occurs in this codebase: a panic
/// location, an `io::Error` about a media file, a Windows config path, a NAS
/// share, the output process's socket, an update-ring URL. The pairs the
/// [`fixtures`] test writes out are what E4's `seam-probe-stage.test.ts` feeds
/// to `ABSOLUTE_PATH_RE`, so a change to either side's idea of "clean" fails a
/// test instead of 400-ing a real crash report in the field.
pub const FIXTURE_HOME: &str = "/Users/ola";

/// `(name, input)` pairs for [`FIXTURE_CASES`]. Kept as data so the emitted
/// JSON is stable under reordering of the module's functions.
pub const FIXTURE_CASES: &[(&str, &str)] = &[
    (
        "panic_message_plain",
        "called `Option::unwrap()` on a `None` value",
    ),
    (
        "panic_location_relative_survives",
        "src/output/process.rs:301:9",
    ),
    (
        "home_directory_becomes_placeholder",
        "thread panicked at /Users/ola/Library/Application Support/app.sundaystage/sundaystage.db",
    ),
    (
        "embedded_in_debug_value",
        "Some(\"/Users/ola/Musikk/salme.mp4\")",
    ),
    (
        "another_users_home",
        "open(/home/kari/media/logo.png) failed: No such file or directory (os error 2)",
    ),
    (
        "windows_roaming_path",
        "C:\\Users\\Ola\\AppData\\Roaming\\SundayStage\\output_config.json not found",
    ),
    ("unc_share", "\\\\NAS\\felles\\media\\bg.mp4 unreachable"),
    (
        "output_socket_in_tmp",
        "bind /tmp/sundaystage-output-main-0.sock failed",
    ),
    (
        "external_volume",
        "disk full while writing /Volumes/Media/Sunday/service.json",
    ),
    (
        "half_washed_tilde_path",
        "kunne ikke åpne ~/Opptak/gudstjeneste.wav",
    ),
    (
        "url_survives",
        "https://updates.sundaysuite.app/v1/update/sundaystage/stable returned 500",
    ),
    ("bare_slash_in_prose", "vis logo og / eller svart"),
    (
        "norwegian_multibyte",
        "kunne ikke lese sangbøkene på skjermen — æøå",
    ),
    (
        "mixed_paths_one_line",
        "copy /Users/ola/a.png -> /Volumes/Backup/b.png (=failed)",
    ),
];

/// The [`crate::telemetry::crash_ring`] message cap — the fixtures pin the
/// truncation convention against it, and E4 pins the same number server-side.
pub const FIXTURE_MESSAGE_MAX: usize = crate::telemetry::MESSAGE_MAX_CHARS;

#[cfg(test)]
mod tests {
    use super::*;

    fn scrub(text: &str) -> String {
        free_text(text, Some(FIXTURE_HOME), FIXTURE_MESSAGE_MAX)
    }

    // ── the half-washed-path bug, first ──────────────────────────────────────

    #[test]
    fn the_whole_path_token_goes_including_the_filename() {
        // The SundayRec 2026-08-08 bug in one assertion: substring-replacing the
        // home directory and then tokenising on whitespace left
        // `~/Opptak/gudstjeneste.wav` on the wire, because the token began with
        // `S`. The filename must not survive.
        let out = scrub("kunne ikke åpne Some(\"/Users/ola/Opptak/gudstjeneste.wav\")");
        assert_eq!(out, "kunne ikke åpne Some(\"<path>\")");
        assert!(!out.contains("gudstjeneste"));
        assert!(!out.contains('~'));
    }

    #[test]
    fn a_tilde_path_is_consumed_even_though_scrub_paths_made_it() {
        // `scrub_paths` runs first and produces `~/…`; the second pass must
        // treat that as a path start, not as prose.
        assert_eq!(scrub("~/Opptak/x.wav"), "<path>");
        assert_eq!(strip_absolute_paths("~\\Opptak\\x.wav"), "<path>");
    }

    #[test]
    fn a_user_placeholder_spliced_mid_path_does_not_end_the_run() {
        // `scrub_paths` turns another account's path into `/home/<user>/…`; the
        // `>` in the placeholder is a delimiter, so without the explicit
        // swallow the tail (`/Opptak/x.wav`) would survive.
        let once = scrub_paths("/home/kari/Opptak/x.wav", Some(FIXTURE_HOME));
        assert_eq!(once, "/home/<user>/Opptak/x.wav");
        assert_eq!(strip_absolute_paths(&once), "<path>");
    }

    // ── what must survive ────────────────────────────────────────────────────

    #[test]
    fn relative_paths_and_urls_survive() {
        // A panic location is the most useful field a crash record carries and
        // is identical on every machine.
        assert_eq!(
            scrub("src/output/process.rs:301:9"),
            "src/output/process.rs:301:9"
        );
        assert_eq!(
            scrub("https://updates.sundaysuite.app/v1/update/sundaystage/stable returned 500"),
            "https://updates.sundaysuite.app/v1/update/sundaystage/stable returned 500"
        );
        // A lone slash in Norwegian prose is not a path.
        assert_eq!(
            scrub("vis logo og / eller svart"),
            "vis logo og / eller svart"
        );
        // Neither is a bare backslash pair in prose.
        assert_eq!(strip_absolute_paths("a \\\\ b"), "a \\\\ b");
    }

    // ── the shapes that must go ──────────────────────────────────────────────

    #[test]
    fn every_absolute_shape_is_replaced() {
        for (input, want) in [
            ("C:\\Users\\Ola\\AppData\\x.json", "<path>"),
            ("c:/users/ola/x.json", "<path>"),
            ("\\\\NAS\\felles\\bg.mp4", "<path>"),
            ("/tmp/sundaystage-output-main-0.sock", "<path>"),
            ("/Volumes/Media/Sunday/service.json", "<path>"),
            ("/private/var/folders/xy/T/x", "<path>"),
            ("/Applications/SundayStage.app/Contents/MacOS/x", "<path>"),
        ] {
            assert_eq!(strip_absolute_paths(input), want, "input: {input}");
        }
    }

    #[test]
    fn scrubbing_is_idempotent() {
        for (_, input) in FIXTURE_CASES {
            let once = scrub(input);
            assert_eq!(scrub(&once), once, "not idempotent: {input}");
        }
    }

    #[test]
    fn the_home_directory_never_survives_in_any_form() {
        for (_, input) in FIXTURE_CASES {
            let out = scrub(input);
            assert!(
                !out.contains(FIXTURE_HOME),
                "home leaked from {input}: {out}"
            );
            assert!(
                !out.contains("/Users/"),
                "user root leaked from {input}: {out}"
            );
            assert!(
                !out.contains("\\Users\\"),
                "user root leaked from {input}: {out}"
            );
        }
    }

    #[test]
    fn a_trailing_separator_on_home_collapses_cleanly() {
        assert_eq!(
            free_text("/Users/ola/x", Some("/Users/ola/"), 200),
            "<path>"
        );
        // An empty/whitespace home is ignored rather than replacing everything.
        assert_eq!(free_text("hei", Some("   "), 200), "hei");
        assert_eq!(free_text("hei", None, 200), "hei");
    }

    // ── truncation ───────────────────────────────────────────────────────────

    #[test]
    fn truncation_is_max_plus_one_with_the_ellipsis_last() {
        // The ellipsis-+1 convention, shared with the Worker's validator: cut at
        // `max`, THEN push `…`. A validator that rejected `max + 1` would 400
        // every long message and (law 2) drop it silently.
        let long = "æ".repeat(FIXTURE_MESSAGE_MAX + 50);
        let out = truncate_with_ellipsis(&long, FIXTURE_MESSAGE_MAX);
        assert_eq!(out.chars().count(), FIXTURE_MESSAGE_MAX + 1);
        assert!(out.ends_with('…'));
        // Multi-byte input is never cut mid-character.
        assert!(out.chars().take(FIXTURE_MESSAGE_MAX).all(|c| c == 'æ'));
        // At or below the cap nothing is added.
        assert_eq!(truncate_with_ellipsis("kort", FIXTURE_MESSAGE_MAX), "kort");
        let exact = "a".repeat(FIXTURE_MESSAGE_MAX);
        assert_eq!(truncate_with_ellipsis(&exact, FIXTURE_MESSAGE_MAX), exact);
    }

    // ── the cross-repo fixture file ──────────────────────────────────────────

    /// Emit `src-tauri/telemetry-scrub-fixtures.json`: this scrubber's EXACT
    /// output for every [`FIXTURE_CASES`] entry.
    ///
    /// The file is committed. E4's `seam-probe-stage.test.ts` reads it and feeds
    /// each `scrubbed` string to the Worker's `ABSOLUTE_PATH_RE`, which is the
    /// only way the two repositories can disagree about "clean" and find out
    /// before a real crash report is 400-ed in the field.
    ///
    /// Writing (rather than merely asserting) is deliberate: regenerating is
    /// `cargo test fixtures`, and a diff in `git status` is the review signal.
    #[test]
    fn fixtures() {
        let cases: Vec<serde_json::Value> = FIXTURE_CASES
            .iter()
            .map(|(name, input)| {
                serde_json::json!({
                    "name": name,
                    "input": input,
                    "scrubbed": scrub(input),
                })
            })
            .collect();
        // One long case pins the truncation convention across the seam too.
        let long_input = format!(
            "{} {}",
            "æ".repeat(FIXTURE_MESSAGE_MAX + 40),
            "/Users/ola/x"
        );
        let doc = serde_json::json!({
            "note": concat!(
                "Generated by `cargo test fixtures` in sundaystage ",
                "src-tauri/src/telemetry/scrub.rs. Do not edit by hand. ",
                "E4 feeds every `scrubbed` value to the sunday-telemetry ",
                "validator: each must pass ABSOLUTE_PATH_RE and the ",
                "message cap unchanged."
            ),
            "schema": 1,
            "home": FIXTURE_HOME,
            "residual": concat!(
                "KNOWN AND DELIBERATE: a path run stops at the first ",
                "whitespace, so a folder or filename containing a SPACE ",
                "leaves its tail behind — see `home_directory_becomes_",
                "placeholder`, where `Application Support/...` leaves ",
                "`Support/app.sundaystage/sundaystage.db`. The tail is a ",
                "RELATIVE fragment and still passes ABSOLUTE_PATH_RE, and it ",
                "names this app's own layout rather than the operator. Do not ",
                "'fix' this server-side: every closure guesses, and eating ",
                "spaces would swallow the Norwegian sentence a crash message ",
                "is identified by."
            ),
            "messageMaxChars": FIXTURE_MESSAGE_MAX,
            "pathPlaceholder": PATH_PLACEHOLDER,
            "userPlaceholder": USER_PLACEHOLDER,
            "cases": cases,
            "truncation": {
                "input": long_input,
                "scrubbed": scrub(&long_input),
            },
        });
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("telemetry-scrub-fixtures.json");
        let body = format!(
            "{}\n",
            serde_json::to_string_pretty(&doc).expect("fixture doc serialises")
        );
        std::fs::write(&path, &body).expect("fixture file is writable");

        // The properties E4 will assert server-side, asserted here too so a
        // regression fails in BOTH repositories rather than only in the one
        // that happens to run first.
        for case in doc["cases"].as_array().expect("cases array") {
            let out = case["scrubbed"].as_str().expect("scrubbed string");
            assert!(!out.contains(FIXTURE_HOME), "{case}");
            assert!(!out.contains("/Users/"), "{case}");
            assert!(!out.contains("/Volumes/"), "{case}");
            assert!(!out.contains("\\Users\\"), "{case}");
        }
        let truncated = doc["truncation"]["scrubbed"].as_str().expect("string");
        assert_eq!(truncated.chars().count(), FIXTURE_MESSAGE_MAX + 1);
        assert!(truncated.ends_with('…'));
    }
}
