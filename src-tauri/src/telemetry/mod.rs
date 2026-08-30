//! E3/E5 — local observation, and the client that can offer it up.
//!
//! Everything that goes on the wire exists here FIRST: already scrubbed, already
//! ring-bounded, and provably unable to touch the live path. A stage that cannot
//! be observed cannot be trusted; a stage whose observation can freeze the
//! projector is worse than one that is not observed.
//!
//! ## Module map
//!
//! **E3 — the local ground floor. None of this sends anything, ever.**
//!
//! * [`scrub`] — the path scrubber. Law 2 ("content never leaves the machine")
//!   in code, applied BEFORE anything reaches disk and again when it is read
//!   back. Also emits the cross-repository fixture file E4 probes with.
//! * [`crash_ring`] — a bounded ring of structured, scrubbed crash records
//!   under `<app-data>/crashes/`, fed by the process panic hook and by the
//!   renderer's `window.onerror` / `unhandledrejection` handlers.
//! * [`native_crash`] (A6) — the one crash the panic hook cannot see: a
//!   segfault, an abort, an out-of-memory kill. A **signal source**, never a
//!   crash reporter — it carries a signal type, an offset inside our own image
//!   and a thread role, and it writes no minidump, because a minidump is memory
//!   and this process's memory holds the verse on the screen.
//! * [`logfile`] — a size-capped rotating file log, because a shipped app has
//!   no console, plus the `log_tail` reader that scrubs a second time.
//! * [`counters`] — a closed set of 19 counter names, incremented atomically
//!   and folded into SQLite when nothing is live.
//! * [`quality`] — one health row per live session, collected through an API
//!   that is non-blocking BY TYPE and drained only when `AppState.live` is
//!   `None`. This is the module law 1 is about.
//!
//! **E5 — the client half. Present, tested, and INERT in this stage.**
//!
//! * [`consent`] — the three-state machine. `NeverAsked` is not `Denied`.
//! * [`client`] — its shell: the record, the lazily minted install id, the
//!   parked deletions, the drain and the preview.
//! * [`payload`] — the one builder that serves the wire, the preview and (from
//!   E6) the report dialog.
//! * [`outbox`] — the bounded queue, its backoff ladder, and the consent gate
//!   as a pure function.
//! * [`config`] — where the endpoint would be, if this build had one.
//! * [`http_sender`] — the socket. **NETWORK-UNVERIFIED.**
//! * [`sender`] — the supervised pump, and the live gate at the top of its beat.
//!
//! ## Why E5 ships inert, and why that is the point
//!
//! There is no consent UI until E6. With no way to answer the question the
//! record stays absent, `evaluate(None)` is `NeverAsked`, no install id is ever
//! minted, `client::drain` returns on its first line and no pump is spawned. On
//! top of that, [`config::TelemetryEndpoint::resolve`] returns `None` in every
//! build without the two build-time variables — which is every build here. Two
//! independent reasons nothing is sent, either of which would be sufficient.
//!
//! ## The one exception to law 1
//!
//! The panic hook writes a scrubbed file synchronously. At panic time there is
//! no live path left to protect in this process, and the output child survives
//! as its own process holding the last frame (see `output::process`). Every
//! OTHER write in this family is gated on "no live session".

pub mod client;
pub mod config;
pub mod consent;
pub mod counters;
pub mod crash_ring;
pub mod http_sender;
pub mod logfile;
pub mod native_crash;
pub mod outbox;
pub mod payload;
pub mod quality;
pub mod scrub;
pub mod sender;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The record schema written into the crash ring. Bumped when a field changes
/// MEANING, so a later reader can tell an old file from a new one.
pub const RECORD_SCHEMA: u32 = 1;

/// Cap for a crash message. 200 characters is where a panic's IDENTITY lives
/// (``called `Option::unwrap()` on a `None` value``, `index out of bounds: the
/// len is 3 but the index is 7`); past that a message is a formatted dump. The
/// wire cap and the local cap are deliberately the SAME number so the file on
/// disk is byte-identical to what a later release would send — no second
/// truncation pass that could disagree with the Worker's.
pub const MESSAGE_MAX_CHARS: usize = 200;

/// Cap for a `file:line:col`.
pub const LOCATION_MAX_CHARS: usize = 120;

/// Cap for a task / component name. Every value today is a `&'static str`.
pub const TASK_MAX_CHARS: usize = 64;

/// The operating system, as a CLOSED set. `std::env::consts::OS` is a `&str`
/// with no compile-time guarantee about its contents, so it is mapped here
/// rather than forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryOs.ts")]
#[serde(rename_all = "snake_case")]
pub enum TelemetryOs {
    Macos,
    Windows,
    Linux,
    Other,
}

impl TelemetryOs {
    /// This machine's OS.
    pub fn current() -> Self {
        Self::from_consts(std::env::consts::OS)
    }

    /// Map a `std::env::consts::OS` string onto the closed set. Pure, so the
    /// mapping is testable without cross-compiling.
    pub fn from_consts(os: &str) -> Self {
        match os {
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            _ => Self::Other,
        }
    }
}

/// The CPU architecture, as a CLOSED set — same reasoning as [`TelemetryOs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryArch.ts")]
pub enum TelemetryArch {
    #[serde(rename = "x86_64")]
    X86_64,
    #[serde(rename = "aarch64")]
    Aarch64,
    #[serde(rename = "other")]
    Other,
}

impl TelemetryArch {
    /// This build's architecture.
    pub fn current() -> Self {
        Self::from_consts(std::env::consts::ARCH)
    }

    /// Map a `std::env::consts::ARCH` string onto the closed set.
    pub fn from_consts(arch: &str) -> Self {
        match arch {
            "x86_64" => Self::X86_64,
            "aarch64" | "arm64" => Self::Aarch64,
            _ => Self::Other,
        }
    }
}

/// Wall-clock milliseconds since the epoch.
///
/// Deliberately NOT [`crate::db::now_ms`], which `expect`s on a pre-epoch clock:
/// this one is called from the panic hook, where a panic would abort the
/// process. A broken clock yields 0 rather than a hard kill.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// This build's version, as the crash/quality records carry it.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_maps_onto_the_closed_set() {
        assert_eq!(TelemetryOs::from_consts("macos"), TelemetryOs::Macos);
        assert_eq!(TelemetryOs::from_consts("windows"), TelemetryOs::Windows);
        assert_eq!(TelemetryOs::from_consts("linux"), TelemetryOs::Linux);
        // An OS nobody has shipped this on is `other`, never a raw string.
        assert_eq!(TelemetryOs::from_consts("freebsd"), TelemetryOs::Other);
        assert_eq!(TelemetryOs::from_consts(""), TelemetryOs::Other);
        // …and the running machine is always one of the four.
        assert!(matches!(
            TelemetryOs::current(),
            TelemetryOs::Macos | TelemetryOs::Windows | TelemetryOs::Linux | TelemetryOs::Other
        ));
    }

    #[test]
    fn os_serialises_snake_case_for_the_wire() {
        let json = serde_json::to_string(&TelemetryOs::Macos).expect("serialises");
        assert_eq!(json, "\"macos\"");
    }

    #[test]
    fn arch_maps_onto_the_closed_set() {
        assert_eq!(TelemetryArch::from_consts("x86_64"), TelemetryArch::X86_64);
        assert_eq!(
            TelemetryArch::from_consts("aarch64"),
            TelemetryArch::Aarch64
        );
        // Tauri's own target triples say `aarch64`, but `arm64` is what several
        // toolchains call the same silicon; both must be one value on the wire.
        assert_eq!(TelemetryArch::from_consts("arm64"), TelemetryArch::Aarch64);
        assert_eq!(TelemetryArch::from_consts("riscv64"), TelemetryArch::Other);
        assert_eq!(TelemetryArch::from_consts(""), TelemetryArch::Other);
        assert!(matches!(
            TelemetryArch::current(),
            TelemetryArch::X86_64 | TelemetryArch::Aarch64 | TelemetryArch::Other
        ));
        assert_eq!(
            serde_json::to_string(&TelemetryArch::X86_64).expect("serialises"),
            "\"x86_64\""
        );
    }

    #[test]
    fn now_ms_never_panics_and_is_plausible() {
        // The panic hook calls this; a panic inside a panic hook aborts.
        let t = now_ms();
        assert!(t > 1_700_000_000_000, "expected a post-2023 clock, got {t}");
    }
}
