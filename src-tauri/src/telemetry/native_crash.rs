//! A6 — the hard crash as a **signal source**, never as a crash report.
//!
//! ## The hole this fills
//!
//! Everything else in [`crate::telemetry`] assumes the process is still alive
//! long enough to write something down. A panic unwinds and the hook runs; a
//! renderer error reaches an IPC command; an output child dying is watched from
//! the outside. A **segfault, an abort or an out-of-memory kill leaves nothing**:
//! the process is gone before any Rust runs. The volunteer's experience is that
//! SundayStage "just disappeared" in the middle of the service, and we never
//! learn that it happened at all.
//!
//! ## Why this is NOT a crash reporter
//!
//! The obvious answer is a minidump, and it is the one answer we may never give.
//! Law 2 of both programmes is that **content never leaves the machine** — song
//! lyrics, titles, service names, file paths. A minidump *is* process memory, and
//! at the moment of a crash this process's memory holds the verse that was on the
//! congregation's screen. There is no scrubbing pass that makes that safe, because
//! there is no structure to scrub: it is a byte image of everything. So:
//!
//! * **no minidump is written, ever** — not even locally, not even unsent. A file
//!   that cannot be sent is still a file that can be attached to an email;
//! * **no backtrace**, because a backtrace names every source file it walked
//!   through, which on a machine built from source is a listing of someone's disk;
//! * **no registers**, because a register can hold a pointer into a string;
//! * **no faulting address value**, only a three-way classification of it;
//! * **no module NAMES**, because a module name is a filename. See [`Site`].
//!
//! What is left is the signal itself, and that turns out to be enough to answer
//! the questions an aggregate can actually act on: *does this fleet crash hard,
//! how, on which build, and is it our code or somebody else's?*
//!
//! ## Exactly what a native crash puts on the wire
//!
//! One record in the EXISTING `crashes[]` array (no new transport, no new
//! endpoint, no schema change — see "Worker" below), built through
//! [`crash_ring::build_entry`] so it goes through the same scrubber and the same
//! caps as every other crash record:
//!
//! | field | value | why it cannot be content |
//! | --- | --- | --- |
//! | `kind` | `other` | a closed enum the Worker already accepts |
//! | `message` | `native crash: sig=segv code=1 fault=null` | every token is a compile-time constant from [`NativeSignal`]/[`FaultAddress`] plus one integer |
//! | `location` | `app+0x1a2b3c` / `foreign` | `app`/`foreign` are literals; the offset is `pc − image_base`, an integer |
//! | `task` | `main-thread` / `other-thread` / `unknown-thread` | a three-value literal set |
//! | `at` | the pending file's mtime | a clock reading |
//! | `appVersion` | the version that crashed | already on every record; filtered to `[0-9A-Za-z.+-]` |
//! | `os` | closed enum | already on every record |
//! | `backtracePresent` | `false` | we capture none |
//!
//! There is no free-text field anywhere in that list. The record is produced by
//! [`NativeCrash::write_line`] from a struct of enums and two integers, and
//! [`tests::the_record_can_only_spell_its_own_vocabulary`] sweeps every
//! reachable combination and asserts the bytes stay inside a fixed character
//! class. A field that *could* carry content was not made safe — it was not
//! collected.
//!
//! ## The signal-handler discipline
//!
//! The callback runs in a compromised context: an async-signal-safe world on
//! Unix, a Mach exception-handler thread on macOS, an unhandled-exception filter
//! on Windows. So the callback:
//!
//! * **allocates nothing.** The record is formatted into a stack buffer of
//!   [`RECORD_MAX_BYTES`]; [`NativeCrash::write_line`] takes `&mut [u8]` and has
//!   no `String`, no `format!`, no `Vec` anywhere in it.
//! * **takes no lock of ours.** Everything it reads was resolved at arm time into
//!   an atomic or a [`OnceLock`] that is only ever set once.
//! * **does no ordinary I/O.** One `write(2)` (Unix) to a file descriptor opened
//!   at arm time. If the file could not be opened, the handler is never installed
//!   at all — a handler with nowhere to write is pure risk.
//! * **cannot panic.** No indexing, no slicing beyond a bounds check, no
//!   `unwrap`. A panic inside a crash handler aborts.
//! * **cannot loop.** The write retries a bounded number of times and gives up.
//! * **runs at most once**, guarded by an [`AtomicBool`] swap.
//!
//! Resolving the record into words — parsing, scrubbing, writing the ring entry —
//! happens on the NEXT startup ([`adopt`]), in ordinary code where allocation is
//! allowed. The handler's whole job is to get ~150 bytes of integers onto a disk
//! that may be about to lose the process.
//!
//! ## Chaining: we must not make a crash worse
//!
//! The callback returns `Handled(false)` **always**. In `crash-handler` that means
//! restore the previous handler and let the signal re-trigger (Unix), or reply
//! `KERN_FAILURE` and hand the exception back to the previously registered ports
//! (macOS), or fall through to the previous unhandled-exception filter (Windows).
//! Consequences we want and would lose by swallowing the crash:
//!
//! * Rust's own stack-overflow handler still prints `thread '…' has overflowed
//!   its stack`;
//! * macOS still writes its own crash report and the OS still reports the app as
//!   having quit unexpectedly;
//! * the exit status still says "killed by SIGSEGV", so anything watching this
//!   process from outside sees the truth.
//!
//! [`tests::a_real_segfault_is_captured_and_still_kills_the_process`] spawns a
//! real child, faults it for real, and asserts BOTH halves: the record exists and
//! the child died of the signal.
//!
//! ## What is deliberately NOT armed
//!
//! The **output child** (`bin/output.rs`) does not arm this. An output child
//! dying is not a crash in the programme's vocabulary — it is a quality signal
//! (`outputChildRestarts`), because the projector kept showing its last frame
//! throughout. That is the crash-isolation design working, and reporting it as a
//! crash would report a success as a failure.
//!
//! ## Consent
//!
//! Nothing here consults consent, and that is correct: capture is local, exactly
//! like the panic ring beside it (see [`crash_ring::RETIRED_FLAG_FILE`]).
//! TRANSMISSION is decided by [`crate::telemetry::consent`] and the crash
//! watermark in [`crate::telemetry::client`], which a native record passes
//! through unchanged because it IS an ordinary ring record by the time anything
//! could send it. In particular a hard crash captured *before* the operator was
//! asked is never sent: granting consent moves the watermark to "now", so
//! everything older than the answer stays on the machine. That seam is pinned by
//! [`tests::a_crash_captured_before_consent_is_never_sent`].
//!
//! ## Worker
//!
//! No schema change, on purpose. `STAGE_CRASH_KINDS` in the deployed Worker is a
//! closed set and a client that invents a value is refused with a 400 — which the
//! outbox drops permanently, without retry (law 3). So a native crash arrives as
//! the existing `other` kind, and the `message` prefix `native crash:` is what an
//! aggregate groups on. Giving it its own `kind` is a Worker-first change for a
//! later stage: Worker deployed and live-verified, THEN a client release.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::telemetry::crash_ring::{self, CrashEntry, CrashKind};
use crate::telemetry::{app_version, now_ms};

/// The pending-record file, in the app-data dir (NOT in the crash ring's
/// directory: it is held open by the handler for the whole run, and unlinking a
/// file a signal handler still holds an fd to would leave the handler writing
/// into a ghost inode).
const PENDING_FILE: &str = "native-crash-pending.txt";

/// The disarm flag file, same shape as `services::update_channel`'s.
const SETTINGS_FILE: &str = "native_crash.json";

/// Longest record [`NativeCrash::write_line`] can produce, with room to spare.
/// The buffer lives on the handler's stack — on Unix that is the alternate
/// signal stack, so it has to stay small enough to fit there during a stack
/// overflow, which is the one crash where the ordinary stack is gone.
pub const RECORD_MAX_BYTES: usize = 256;

/// First token of a record. A file that does not start with this is not ours.
const MAGIC: &str = "sundaystage-native-crash";

/// Format version of the pending record. Bumped if a field changes MEANING, so
/// a newer build adopting an older build's record can tell.
const FORMAT_VERSION: &str = "v1";

/// What a record says when the version does not look like a version.
const VERSION_UNKNOWN: &str = "unknown";

// ─────────────────────────────────────────────────────────────────────────────
//   The vocabulary — every string that can reach the wire is in this section
// ─────────────────────────────────────────────────────────────────────────────

/// What killed the process, as a CLOSED set of compile-time constants.
///
/// Deliberately not the raw signal number or the raw `NTSTATUS`: those travel as
/// the separate integer `code`, and a name we do not recognise becomes
/// [`NativeSignal::Other`] rather than a string we forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeSignal {
    /// Invalid memory access. `SIGSEGV`, `EXC_BAD_ACCESS`, `ACCESS_VIOLATION`.
    Segv,
    /// Bus error / misaligned or unmapped access. `SIGBUS`.
    Bus,
    /// Illegal instruction. `SIGILL`, `EXC_BAD_INSTRUCTION`.
    Ill,
    /// Arithmetic fault. `SIGFPE`, `EXC_ARITHMETIC`.
    Fpe,
    /// Deliberate abort — `abort()`, a failed assertion, Rust's abort-on-panic,
    /// a heap-corruption fast-fail. Also where a Linux OOM-killed process lands
    /// when it aborts rather than being `SIGKILL`ed outright.
    Abort,
    /// Trap / breakpoint.
    Trap,
    /// Stack exhaustion, where the platform names it separately.
    StackOverflow,
    /// Something this build cannot name. The escape hatch that keeps the set
    /// closed.
    Other,
}

impl NativeSignal {
    /// The wire spelling. One word, lowercase, no separators except `-`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Segv => "segv",
            Self::Bus => "bus",
            Self::Ill => "ill",
            Self::Fpe => "fpe",
            Self::Abort => "abort",
            Self::Trap => "trap",
            Self::StackOverflow => "stack-overflow",
            Self::Other => "other",
        }
    }

    /// Parse a wire spelling. Anything outside the set is `None` — the
    /// allow-list is the point, on the way in as well as on the way out.
    pub fn from_wire(raw: &str) -> Option<Self> {
        Some(match raw {
            "segv" => Self::Segv,
            "bus" => Self::Bus,
            "ill" => Self::Ill,
            "fpe" => Self::Fpe,
            "abort" => Self::Abort,
            "trap" => Self::Trap,
            "stack-overflow" => Self::StackOverflow,
            "other" => Self::Other,
            _ => return None,
        })
    }

    /// Every value, for the sweeps.
    pub const ALL: [Self; 8] = [
        Self::Segv,
        Self::Bus,
        Self::Ill,
        Self::Fpe,
        Self::Abort,
        Self::Trap,
        Self::StackOverflow,
        Self::Other,
    ];
}

/// A CLASSIFICATION of the faulting address — never the address itself.
///
/// The value would be a number, not content, but it would also be useless: ASLR
/// makes it incomparable between two machines, and between two runs of the same
/// machine. The one thing it genuinely says is whether the pointer was null, and
/// that survives the classification. Anything else is dropped rather than
/// argued about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultAddress {
    /// Exactly zero — a null dereference.
    Null,
    /// Inside the first page. A null dereference through a struct field or an
    /// array index, which reads the same to a developer and differently to a
    /// naive `== 0` check.
    LowPage,
    /// A real address somewhere else. The value is discarded here.
    NonNull,
    /// The platform gave no faulting address for this kind of crash (an abort
    /// has none).
    Unknown,
}

impl FaultAddress {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::LowPage => "low",
            Self::NonNull => "nonnull",
            Self::Unknown => "unknown",
        }
    }

    /// Parse a wire spelling.
    pub fn from_wire(raw: &str) -> Option<Self> {
        Some(match raw {
            "null" => Self::Null,
            "low" => Self::LowPage,
            "nonnull" => Self::NonNull,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    /// Classify a raw faulting address. The only function that ever sees the
    /// value, and it returns four bits of it.
    pub fn classify(addr: u64) -> Self {
        match addr {
            0 => Self::Null,
            // One page. Big enough to cover a null pointer plus a field offset
            // or a small index, small enough that nothing legitimate lives there
            // on any platform we ship.
            1..=4095 => Self::LowPage,
            _ => Self::NonNull,
        }
    }

    /// Every value, for the sweeps.
    pub const ALL: [Self; 4] = [Self::Null, Self::LowPage, Self::NonNull, Self::Unknown];
}

/// Where the program counter was, to the only resolution we are willing to
/// publish.
///
/// **Why there are no module names here.** The obvious design carries the
/// crashing module's basename (`sundaystage`, `libwebkit2gtk-4.1.so.0`,
/// `d3d11.dll`) so an aggregate can say "the crashes are all in the graphics
/// driver". A basename is a filename, and this app's own filename is one an
/// operator can rename; a plug-in's is one we do not control at all. The rule
/// the brief sets is that a field we are unsure about is not collected, so the
/// module collapses to two literals decided by one range check:
///
/// * [`Site::App`] — the program counter was inside SundayStage's own executable
///   image, and the offset is `pc − image_base`. That offset is symbolisable
///   against the matching release binary (`atos`, `llvm-symbolizer`) and equal
///   offsets across machines mean equal crash sites, which is the clustering an
///   aggregate needs.
/// * [`Site::Foreign`] — it was outside. Some library, some driver, some JIT
///   page: we learn it was not our code, which is the actionable half, and we
///   learn nothing about which file it was.
/// * [`Site::Unknown`] — the platform gave no program counter, or the image
///   range could not be resolved at arm time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    /// Inside our own executable image, at this offset from its base.
    App { offset: u64 },
    /// Outside it.
    Foreign,
    /// Not determined.
    Unknown,
}

impl Site {
    /// Every value that has no payload, for the sweeps.
    pub const FIXED: [Self; 2] = [Self::Foreign, Self::Unknown];
}

/// Which thread died, to the only resolution that is a role rather than an id.
///
/// A thread id is a number that means nothing outside the run it came from; the
/// question worth answering is whether the operator's UI thread went down (they
/// watched the window die) or a background one did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadRole {
    /// The process's main thread — the one Tauri drives the window from.
    Main,
    /// Any other thread.
    Other,
    /// The platform would not say.
    Unknown,
}

impl ThreadRole {
    /// The wire spelling. Suffixed so the value reads as a role in the `task`
    /// field, which elsewhere holds names like `workspace`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main-thread",
            Self::Other => "other-thread",
            Self::Unknown => "unknown-thread",
        }
    }

    /// Parse a wire spelling.
    pub fn from_wire(raw: &str) -> Option<Self> {
        Some(match raw {
            "main-thread" => Self::Main,
            "other-thread" => Self::Other,
            "unknown-thread" => Self::Unknown,
            _ => return None,
        })
    }

    /// Every value, for the sweeps.
    pub const ALL: [Self; 3] = [Self::Main, Self::Other, Self::Unknown];
}

// ─────────────────────────────────────────────────────────────────────────────
//   The record
// ─────────────────────────────────────────────────────────────────────────────

/// Everything a hard crash is allowed to say about itself.
///
/// Four enums and one integer. There is no `String` in this struct and no
/// constructor that takes one, which is the structural half of the promise the
/// module docs make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeCrash {
    pub signal: NativeSignal,
    /// The platform's own numeric code — `si_code` on Unix, the Mach exception
    /// code on macOS, the `NTSTATUS` on Windows. An integer, and the only reason
    /// it survives at all is that it separates "null dereference" from
    /// "protection failure" inside one signal.
    pub code: i64,
    pub fault: FaultAddress,
    pub site: Site,
    pub thread: ThreadRole,
}

impl NativeCrash {
    // HANDLER-SAFE-BEGIN
    /// Format the record into `out`, returning how many bytes were written, or
    /// `None` if `out` is too small.
    ///
    /// **This is the function that runs inside the crash handler.** No
    /// allocation, no formatting machinery, no panics: every write goes through
    /// [`Cursor`], which bounds-checks and then gives up rather than indexing.
    pub fn write_line(&self, version: &str, out: &mut [u8]) -> Option<usize> {
        let mut c = Cursor::new(out);
        c.str(MAGIC);
        c.byte(b' ');
        c.str(FORMAT_VERSION);
        c.str(" sig=");
        c.str(self.signal.as_str());
        c.str(" code=");
        c.int(self.code);
        c.str(" fault=");
        c.str(self.fault.as_str());
        c.str(" site=");
        match self.site {
            Site::App { offset } => {
                c.str("app+0x");
                c.hex(offset);
            }
            Site::Foreign => c.str("foreign"),
            Site::Unknown => c.str("unknown"),
        }
        c.str(" thread=");
        c.str(self.thread.as_str());
        c.str(" ver=");
        c.version(version);
        c.byte(b'\n');
        c.finish()
    }
    // HANDLER-SAFE-END

    /// Parse a record written by [`Self::write_line`]. Returns the crash and the
    /// version that crashed.
    ///
    /// Runs at startup, in ordinary code. Every token is checked against the
    /// closed sets above, so a hand-edited or truncated file yields `None`
    /// rather than something the caller has to distrust.
    pub fn parse_line(raw: &str) -> Option<(Self, String)> {
        let mut words = raw.trim().split(' ');
        if words.next()? != MAGIC || words.next()? != FORMAT_VERSION {
            return None;
        }

        let (mut signal, mut code, mut fault, mut site, mut thread, mut version) =
            (None, None, None, None, None, None);
        for word in words {
            let (key, value) = word.split_once('=')?;
            match key {
                "sig" => signal = Some(NativeSignal::from_wire(value)?),
                "code" => code = Some(value.parse::<i64>().ok()?),
                "fault" => fault = Some(FaultAddress::from_wire(value)?),
                "site" => site = Some(parse_site(value)?),
                "thread" => thread = Some(ThreadRole::from_wire(value)?),
                // Re-checked on the way in: the file sits on a disk anything can
                // edit, so the writer's guarantee is not the reader's.
                "ver" => version = Some(normalize_version(value)),
                // An unknown key is a record from a build that knows more than
                // this one. Refusing the whole record would throw away the
                // fields we DO understand, so it is skipped.
                _ => {}
            }
        }

        Some((
            Self {
                signal: signal?,
                code: code?,
                fault: fault?,
                site: site?,
                thread: thread?,
            },
            version?,
        ))
    }

    /// The `message` this crash contributes to the ring record.
    fn message(&self) -> String {
        format!(
            "native crash: sig={} code={} fault={}",
            self.signal.as_str(),
            self.code,
            self.fault.as_str()
        )
    }

    /// The `location` this crash contributes, or `None` when the site is
    /// unknown — an empty-ish placeholder would be a field pretending to know
    /// something.
    fn location(&self) -> Option<String> {
        match self.site {
            Site::App { offset } => Some(format!("app+0x{offset:x}")),
            Site::Foreign => Some("foreign".to_string()),
            Site::Unknown => None,
        }
    }

    /// Project onto the ring's record type.
    ///
    /// Goes through [`crash_ring::build_entry`] rather than constructing a
    /// [`CrashEntry`] by hand, so a native record is scrubbed and capped by the
    /// exact same code as a panic — one scrubber, not two that can drift. The
    /// app version is then replaced with the version that CRASHED, which is
    /// often not the version doing the adopting: an app that hard-crashes on
    /// launch gets updated, and the record outlives the build that wrote it.
    pub fn to_entry(&self, version: &str, at: i64) -> CrashEntry {
        let mut entry = crash_ring::build_entry(
            CrashKind::Other,
            &self.message(),
            self.location().as_deref(),
            Some(self.thread.as_str()),
            at,
            crate::telemetry::scrub::home_dir().as_deref(),
        );
        entry.app_version = normalize_version(version);
        entry
    }
}

/// Parse the `site=` value.
fn parse_site(raw: &str) -> Option<Site> {
    match raw {
        "foreign" => Some(Site::Foreign),
        "unknown" => Some(Site::Unknown),
        _ => {
            let offset = raw.strip_prefix("app+0x")?;
            Some(Site::App {
                offset: u64::from_str_radix(offset, 16).ok()?,
            })
        }
    }
}

/// Whether `raw` has the SHAPE of a version, rather than merely the characters
/// of one.
///
/// The version is the only field in the record whose value is not a `match` over
/// an enum, so it is the only place a string could slip in — and the first draft
/// of this module filtered characters instead of checking shape. The sweep in
/// [`tests::the_record_can_only_spell_its_own_vocabulary`] caught it immediately:
/// filtering `/` out of `/Users/ola/Musikk/salme.mp4` leaves `UsersolaMusikk
/// salme.mp4`, which is every word of a path with the separators removed. A
/// filter cannot tell a version from a sentence; a shape can.
///
/// So: three numeric groups, optionally a prerelease, nothing else. Anything
/// that fails becomes [`VERSION_UNKNOWN`] — a constant, not a salvaged remnant.
/// `CARGO_PKG_VERSION` is a compile-time constant that always passes, which is
/// exactly why the check has to be here rather than trusted: the pending file is
/// read back off a disk, and the writer's guarantees are not the reader's.
///
/// Pure and allocation-free, because the crash handler calls it too.
// HANDLER-SAFE-BEGIN
fn version_is_wellformed(raw: &str) -> bool {
    // `0.7.0-beta.10` is 13; nothing legitimate is close to the cap.
    if raw.is_empty() || raw.len() > 24 {
        return false;
    }
    let (core, pre) = match raw.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (raw, None),
    };

    let mut groups = 0;
    for group in core.split('.') {
        if group.is_empty() || group.len() > 4 || !group.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        groups += 1;
    }
    if groups != 3 {
        return false;
    }

    match pre {
        None => true,
        // Lowercase only, and short: `beta.1`, `rc.2`. Uppercase is where a
        // proper noun would live, and 12 characters is not a sentence.
        Some(pre) => {
            !pre.is_empty()
                && pre.len() <= 12
                && pre
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
        }
    }
}
// HANDLER-SAFE-END

/// The version as it may be recorded: itself if it looks like a version,
/// [`VERSION_UNKNOWN`] otherwise.
fn normalize_version(raw: &str) -> String {
    if version_is_wellformed(raw) {
        raw.to_string()
    } else {
        VERSION_UNKNOWN.to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The allocation-free cursor the handler formats through
// ─────────────────────────────────────────────────────────────────────────────

/// A bounds-checked write head over a byte buffer.
///
/// Every method is a no-op once the buffer is full, and [`Cursor::finish`]
/// reports the overflow rather than silently truncating: a half-written record
/// would parse as garbage on the next startup, which is worse than no record.
// HANDLER-SAFE-BEGIN
struct Cursor<'a> {
    buf: &'a mut [u8],
    at: usize,
    overflowed: bool,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self {
            buf,
            at: 0,
            overflowed: false,
        }
    }

    fn byte(&mut self, b: u8) {
        match self.buf.get_mut(self.at) {
            Some(slot) => {
                *slot = b;
                self.at += 1;
            }
            None => self.overflowed = true,
        }
    }

    fn str(&mut self, s: &str) {
        for b in s.as_bytes() {
            self.byte(*b);
        }
    }

    /// A signed decimal integer, written without `format!`.
    fn int(&mut self, mut v: i64) {
        if v < 0 {
            self.byte(b'-');
        }
        // Twenty digits covers `i64::MIN`. Built backwards, then emitted.
        let mut digits = [0u8; 20];
        let mut n = 0;
        loop {
            // Take the remainder off the NEGATIVE side so `i64::MIN` — whose
            // absolute value does not fit in an `i64` — cannot overflow.
            let rem = if v < 0 { -(v % 10) } else { v % 10 };
            if n < digits.len() {
                digits[n] = b'0' + (rem as u8);
                n += 1;
            }
            v /= 10;
            if v == 0 {
                break;
            }
        }
        self.emit_reversed(&digits, n);
    }

    /// An unsigned lowercase hex integer, without a `0x` prefix.
    fn hex(&mut self, v: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut digits = [0u8; 16];
        let mut n = 0;
        let mut v = v;
        loop {
            if let (Some(slot), Some(d)) = (digits.get_mut(n), HEX.get((v & 0xf) as usize)) {
                *slot = *d;
                n += 1;
            }
            v >>= 4;
            if v == 0 {
                break;
            }
        }
        self.emit_reversed(&digits, n);
    }

    /// Emit `digits[..n]` back to front. Every read is a bounds-checked `get`,
    /// because an out-of-range index in a crash handler is a panic, and a panic
    /// in a crash handler aborts the process it was trying to describe.
    fn emit_reversed(&mut self, digits: &[u8], mut n: usize) {
        while n > 0 {
            n -= 1;
            if let Some(d) = digits.get(n) {
                self.byte(*d);
            }
        }
    }

    /// The version, or [`VERSION_UNKNOWN`] when it does not have the shape of
    /// one. All-or-nothing on purpose: a partially salvaged version is a
    /// partially salvaged string, which is the one thing this record may not
    /// carry. See [`version_is_wellformed`].
    fn version(&mut self, v: &str) {
        if version_is_wellformed(v) {
            self.str(v);
        } else {
            self.str(VERSION_UNKNOWN);
        }
    }

    fn finish(self) -> Option<usize> {
        (!self.overflowed).then_some(self.at)
    }
}
// HANDLER-SAFE-END

// ─────────────────────────────────────────────────────────────────────────────
//   Arm-time state. Everything the handler reads lives here.
// ─────────────────────────────────────────────────────────────────────────────

/// The pending file, held open for the process's whole life so the handler never
/// has to open anything. Leaked into a `OnceLock` on purpose: closing it while a
/// handler could still fire would be a use-after-close in a signal handler.
static PENDING: OnceLock<std::fs::File> = OnceLock::new();

/// Our own executable image's code range, resolved once at arm time.
/// `IMAGE_END == 0` means "not resolved", which yields [`Site::Unknown`].
static IMAGE_BASE: AtomicUsize = AtomicUsize::new(0);
static IMAGE_END: AtomicUsize = AtomicUsize::new(0);

/// The main thread's platform id, captured at arm time. `0` means unknown.
static MAIN_THREAD: AtomicU64 = AtomicU64::new(0);

/// One record per process. A second crash while handling the first must not
/// append a second line to the file.
static WROTE: AtomicBool = AtomicBool::new(false);

/// The installed handler. Held so [`disarm`] can drop it, which restores the
/// previous handlers and exception ports.
static HANDLER: parking_lot::Mutex<Option<crash_handler::CrashHandler>> =
    parking_lot::Mutex::new(None);

/// Classify a program counter against our own image.
///
/// Pure and testable: the range is passed in rather than read from the atomics,
/// so the arithmetic can be proved without arming anything.
// HANDLER-SAFE-BEGIN
pub fn site_for_pc(pc: u64, base: usize, end: usize) -> Site {
    if end == 0 || end <= base || pc == 0 {
        return Site::Unknown;
    }
    let pc = pc as usize;
    if pc >= base && pc < end {
        Site::App {
            offset: (pc - base) as u64,
        }
    } else {
        Site::Foreign
    }
}

/// [`site_for_pc`] against the armed image range.
fn site_for_pc_armed(pc: u64) -> Site {
    site_for_pc(
        pc,
        IMAGE_BASE.load(Ordering::Relaxed),
        IMAGE_END.load(Ordering::Relaxed),
    )
}

/// Compare a platform thread id against the armed main-thread id.
///
/// Pure, same reasoning as [`site_for_pc`]: `0` on either side means the
/// platform would not say, which is [`ThreadRole::Unknown`] — never a guess.
pub fn thread_role(observed: u64, main: u64) -> ThreadRole {
    if observed == 0 || main == 0 {
        ThreadRole::Unknown
    } else if observed == main {
        ThreadRole::Main
    } else {
        ThreadRole::Other
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The handler
// ─────────────────────────────────────────────────────────────────────────────

/// Write `bytes` to the pending file. Called ONLY from the crash handler.
///
/// Unix: a bounded loop around `write(2)`, which is async-signal-safe. Windows:
/// `std::io::Write` on the same `File`, because an unhandled-exception filter is
/// not a signal handler and `File::write` there is a thin `WriteFile` wrapper.
fn write_pending(bytes: &[u8]) {
    let Some(file) = PENDING.get() else { return };

    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;
        let fd = file.as_raw_fd();
        let mut written = 0usize;
        // Bounded: a handler that can loop is a handler that can hang a dying
        // process forever, and nothing downstream would ever see the record.
        for _ in 0..8 {
            if written >= bytes.len() {
                break;
            }
            let rest = &bytes[written..];
            // SAFETY: `fd` is owned by the `OnceLock`, which is never cleared,
            // and `rest` is a live slice for the duration of the call.
            let n = unsafe { libc::write(fd, rest.as_ptr().cast(), rest.len()) };
            if n <= 0 {
                break;
            }
            written += n as usize;
        }
    }

    #[cfg(not(unix))]
    {
        use std::io::Write as _;
        let _ = (&mut &*file).write_all(bytes);
    }
}

/// The crash callback. Everything the module docs promise is enforced here.
fn on_crash(cc: &crash_context::CrashContext) -> crash_handler::CrashEventResult {
    // At most one record per process.
    if !WROTE.swap(true, Ordering::SeqCst) {
        let crash = platform::observe(cc);
        let mut buf = [0u8; RECORD_MAX_BYTES];
        if let Some(n) = crash.write_line(app_version(), &mut buf) {
            // `get` is a bounds check, not an index: a panic here would abort.
            if let Some(record) = buf.get(..n) {
                write_pending(record);
            }
        }
    }

    // ALWAYS false. Restore whatever was there before us and let the crash
    // continue on its way — Rust's stack-overflow message, the OS's own crash
    // reporter, and an exit status that still says the process was killed.
    crash_handler::CrashEventResult::Handled(false)
}
// HANDLER-SAFE-END

// ─────────────────────────────────────────────────────────────────────────────
//   Arming, disarming, and the operator's switch
// ─────────────────────────────────────────────────────────────────────────────

/// The persisted on/off switch for hard-crash capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureSetting {
    enabled: bool,
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}

/// Whether hard-crash capture is enabled. **Defaults to ON**, and an unreadable
/// or malformed file reads as ON.
///
/// The opposite default to every consent question in this family, and for the
/// opposite reason: this switch does not govern sending anything (that is
/// [`crate::telemetry::consent`], and it is default-closed). It governs whether
/// a signal handler is installed at all — a local diagnostic, like the panic
/// ring next to it. Its reason for existing is that a signal handler is the one
/// piece of this system that changes how the process BEHAVES while crashing, so
/// an operator whose rig it upsets must be able to switch it off without waiting
/// for a release.
pub fn capture_enabled(data_dir: &Path) -> bool {
    std::fs::read_to_string(settings_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str::<CaptureSetting>(&raw).ok())
        .map(|s| s.enabled)
        .unwrap_or(true)
}

/// Persist the switch.
pub fn set_capture_enabled(data_dir: &Path, enabled: bool) -> std::io::Result<()> {
    let body = serde_json::to_string(&CaptureSetting { enabled })
        .unwrap_or_else(|_| "{\"enabled\":true}".to_string());
    std::fs::write(settings_path(data_dir), body)
}

/// Whether the handler is installed right now.
pub fn is_armed() -> bool {
    HANDLER.lock().is_some()
}

/// What the Settings card shows and toggles.
///
/// Two booleans, not one, because they can honestly disagree: the operator can
/// have capture switched ON while the handler is NOT armed — the pending file
/// could not be opened, or the platform refused. Reporting only `enabled` would
/// tell an operator their rig is being watched when it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/bindings/NativeCrashStatus.ts")]
#[serde(rename_all = "camelCase")]
pub struct NativeCrashStatus {
    /// The persisted switch.
    pub enabled: bool,
    /// Whether a handler is actually installed in this process right now.
    pub armed: bool,
}

/// The current status.
pub fn status(data_dir: &Path) -> NativeCrashStatus {
    NativeCrashStatus {
        enabled: capture_enabled(data_dir),
        armed: is_armed(),
    }
}

/// An `io::Error` reduced to its KIND, as a `&'static str`.
///
/// `ErrorKind`'s own `Display` is a fixed English phrase per variant, but
/// `io::Error`'s is the OS message — and an OS message can name a path. This
/// keeps the diagnosis and drops the sentence.
fn io_reason(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        std::io::ErrorKind::NotFound => "not-found",
        std::io::ErrorKind::AlreadyExists => "already-exists",
        _ => "io-error",
    }
}

/// Why an arm attempt did or did not install a handler. Returned so the caller
/// can log one honest line instead of guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmOutcome {
    /// The handler is installed.
    Armed,
    /// The operator switched hard-crash capture off.
    Disabled,
    /// The pending file could not be opened, so the handler would have had
    /// nowhere to write. Not installed: a handler that cannot record anything is
    /// all of the risk and none of the benefit.
    NoPendingFile,
    /// The platform refused to install the handler.
    PlatformRefused,
    /// Already armed; this call did nothing.
    AlreadyArmed,
}

/// Install the hard-crash handler.
///
/// Call **after** [`adopt`], which reads the previous run's record: arming
/// truncates the pending file.
pub fn arm(data_dir: &Path) -> ArmOutcome {
    if !capture_enabled(data_dir) {
        return ArmOutcome::Disabled;
    }
    let mut slot = HANDLER.lock();
    if slot.is_some() {
        return ArmOutcome::AlreadyArmed;
    }

    // The file first. If this fails there is nothing to arm FOR.
    if PENDING.get().is_none() {
        let opened = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(pending_path(data_dir));
        match opened {
            Ok(f) => {
                let _ = PENDING.set(f);
            }
            Err(e) => {
                tracing::warn!(
                    kind = %e.kind(),
                    "telemetry: hard-crash capture stayed off — the pending-record file could not \
                     be opened, and a crash handler with nowhere to write is risk without benefit"
                );
                return ArmOutcome::NoPendingFile;
            }
        }
    } else {
        // A second arm in the same process (the operator toggled the switch off
        // and on again). The file is already open; empty it so a stale record
        // from earlier in this run cannot be adopted twice.
        if let Some(f) = PENDING.get() {
            let _ = f.set_len(0);
        }
    }

    platform::capture_image_range(&IMAGE_BASE, &IMAGE_END);
    MAIN_THREAD.store(platform::current_thread_id(), Ordering::Relaxed);
    WROTE.store(false, Ordering::SeqCst);

    // SAFETY: `on_crash` allocates nothing, takes no lock, performs one bounded
    // `write(2)`, cannot panic and cannot loop — see the module docs.
    let event = unsafe { crash_handler::make_crash_event(on_crash) };
    match crash_handler::CrashHandler::attach(event) {
        Ok(h) => {
            *slot = Some(h);
            tracing::info!(
                "telemetry: hard-crash capture armed — signal type, module and offset only; no \
                 minidump is written and none can be"
            );
            ArmOutcome::Armed
        }
        Err(e) => {
            // The REASON, not the error's own words. E3's tracing audit caught
            // the first draft interpolating `{e}` here, and it was right to: the
            // `Io` variant wraps whatever the OS said, and an OS message can
            // name a path. A closed three-value reason answers the same question
            // ("why is this machine not capturing?") with nothing to scrub.
            let reason = match e {
                crash_handler::Error::OutOfMemory => "out-of-memory",
                crash_handler::Error::HandlerAlreadyInstalled => "already-installed",
                crash_handler::Error::Io(ref io) => io_reason(io),
            };
            tracing::warn!(
                reason,
                "telemetry: hard-crash capture could not be armed; the app is unaffected \
                 otherwise"
            );
            ArmOutcome::PlatformRefused
        }
    }
}

/// Remove the handler, restoring whatever was installed before it. Takes effect
/// immediately, so the operator's switch does not need a relaunch to mean
/// something.
pub fn disarm() {
    if HANDLER.lock().take().is_some() {
        tracing::info!("telemetry: hard-crash capture disarmed");
    }
}

/// Where the pending record lives.
pub fn pending_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PENDING_FILE)
}

// ─────────────────────────────────────────────────────────────────────────────
//   Adoption — the previous run's hard crash becomes an ordinary ring record
// ─────────────────────────────────────────────────────────────────────────────

/// Read a pending record left by a previous run, write it into the crash ring,
/// and clear the file. Returns whether a crash was adopted.
///
/// Runs at startup, before [`arm`]. Everything difficult happens here, in
/// ordinary code: parsing, scrubbing, capping, filesystem work.
///
/// The record's timestamp is the pending file's **modification time**, which is
/// when the handler wrote it — the moment of the crash, not the moment of this
/// launch. That matters twice: it keeps the ring in chronological order, and it
/// is the value the consent watermark compares against, so a crash from before
/// the operator was asked stays behind the watermark and is never sent.
pub fn adopt(data_dir: &Path) -> bool {
    let path = pending_path(data_dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    if raw.trim().is_empty() {
        // The ordinary case: the last run exited without crashing, so the file
        // is the empty one `arm` truncated it to.
        return false;
    }

    let at = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or_else(now_ms);

    let adopted = match NativeCrash::parse_line(&raw) {
        Some((crash, version)) => {
            let entry = crash.to_entry(&version, at);
            match crash_ring::write_record(&crash_ring::crash_dir(data_dir), &entry) {
                Ok(()) => {
                    tracing::warn!(
                        sig = crash.signal.as_str(),
                        site = ?crash.site,
                        thread = crash.thread.as_str(),
                        "telemetry: the previous run ended in a hard crash"
                    );
                    true
                }
                Err(e) => {
                    tracing::warn!(
                        kind = %e.kind(),
                        "telemetry: a hard-crash record could not be written to the ring"
                    );
                    false
                }
            }
        }
        None => {
            // A torn or hand-edited file. Say so without quoting it: the whole
            // point of this module is that its bytes are the ones we are willing
            // to keep, and an unparseable file is by definition not those bytes.
            tracing::warn!(
                bytes = raw.len(),
                "telemetry: a pending hard-crash record could not be parsed and was discarded"
            );
            false
        }
    };

    // Cleared either way. A record we could not use must not be adopted again on
    // every launch for the rest of the install's life.
    let _ = std::fs::write(&path, b"");
    adopted
}

// ─────────────────────────────────────────────────────────────────────────────
//   Platform layer
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[path = "native_crash_macos.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "native_crash_linux.rs"]
mod platform;

#[cfg(windows)]
#[path = "native_crash_windows.rs"]
mod platform;

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    use super::*;

    pub(super) fn observe(_cc: &crash_context::CrashContext) -> NativeCrash {
        NativeCrash {
            signal: NativeSignal::Other,
            code: 0,
            fault: FaultAddress::Unknown,
            site: Site::Unknown,
            thread: ThreadRole::Unknown,
        }
    }

    pub(super) fn capture_image_range(_base: &AtomicUsize, _end: &AtomicUsize) {}

    pub(super) fn current_thread_id() -> u64 {
        0
    }
}

#[cfg(test)]
mod tests;
