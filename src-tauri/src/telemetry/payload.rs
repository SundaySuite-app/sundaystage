//! E5 — the payload: the ONLY thing that ever leaves the machine.
//!
//! ## One builder, three readers
//!
//! [`build`] is the single source of the bytes. It serves
//!
//!   * the **wire** — [`super::client::drain`] serialises its result into the
//!     outbox;
//!   * the **preview** — [`super::client::preview_payload`] serialises the SAME
//!     structure for "show me what you send";
//!   * and, from E6, the **problem-report dialog**, which previews the exact
//!     outgoing bytes of a manual report.
//!
//! A second "preview builder" would be worse than no preview at all: it would be
//! a promise about code that never runs. `preview_equals_the_queued_bytes` in
//! the tests below pins the two byte-for-byte.
//!
//! ## Everything here already exists on this machine
//!
//! E3 wrote all of it for the operator's own benefit: the crash ring, the
//! per-session quality rows, the counters. This module reads those, projects
//! them through wire types that cannot express a song title, and assembles one
//! [`StagePayload`]. Nothing here probes anything — building a payload never
//! opens a display, spawns a process or touches the network.
//!
//! ## Draining is idempotent BY WATERMARK
//!
//! Each source has one:
//!
//!   * crashes — a `crash_at` timestamp in [`super::client`]'s state bag; a
//!     record at or below it has been reported;
//!   * quality — the row's own `sent_at` column;
//!   * counters — the row's own `sent_value` column, so what is reported is the
//!     DELTA since the last report and a counter that has not moved contributes
//!     nothing.
//!
//! [`build`] only READS: it returns the payload alongside the [`DrainMarks`]
//! that a successful enqueue should commit. So building a payload twice yields
//! the same bytes twice, and committing once means each datum is sent once.
//!
//! ## Watermarks start at "now"
//!
//! When consent is granted, the crash watermark is set to the current time. A
//! machine that has run services for two years has two years of crash records
//! on disk. Consent given today is consent to share what happens from today;
//! back-filling the archive would technically answer the same question, but it
//! is not what the person said yes to. (The quality and counter watermarks get
//! the same treatment by being marked sent-up-to-now on grant — see
//! `client::on_consent_granted`.)

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::db::repositories::TelemetryRepo;
use crate::error::AppResult;
use crate::telemetry::counters::CounterName;
use crate::telemetry::crash_ring::{self, CrashKind};
use crate::telemetry::quality::{QualityReason, QualityRow, QualityVerdict};
use crate::telemetry::{TelemetryArch, TelemetryOs};

/// The payload schema version. Bumped when a field changes MEANING; adding an
/// optional field is a Worker-first deploy, not a bump (law 3).
pub const PAYLOAD_SCHEMA: u32 = 1;

/// The id a payload carries when no install id has been minted — which is every
/// preview shown before consent is granted. It is the truth: this machine does
/// not have one yet.
pub const NIL_INSTALL_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Cap on a version string.
pub const VERSION_MAX_CHARS: usize = 32;

/// Cap on a language subtag.
pub const LANGUAGE_MAX_CHARS: usize = 8;

/// How many crash records one payload may carry — the ring's own size, so one
/// upload can carry everything it holds.
pub const MAX_CRASHES: usize = 20;

/// How many quality rows one payload may carry.
pub const MAX_QUALITY: usize = 20;

/// How many manual problem reports one payload may carry (E6 fills these).
pub const MAX_REPORTS: usize = 3;

/// The Worker's hard body cap. A payload over this is answered 413.
pub const BODY_MAX_BYTES: usize = 64 * 1024;

/// What [`StagePayload::fit_to_body_cap`] aims for: the cap with 4 kB of head
/// room.
///
/// The margin is not superstition. E4 measured a worst-case payload at
/// **53 505 bytes — 82 % of the cap — using two-byte text**, and the caps are
/// counted in CHARS while the cap is counted in BYTES: the same 20 crash records
/// written in a script whose characters are four bytes each overflow it. The
/// margin also covers the difference between what this client serialises and
/// what an intermediary might add.
///
/// Why this matters more than it looks: **a 413 is dropped exactly as silently
/// as a 400.** [`super::http_sender::classify`] calls both PERMANENT, so an
/// oversized payload is discarded without a retry and without anything the
/// operator could notice. The one defence is not to build one.
pub const BODY_TARGET_BYTES: usize = BODY_MAX_BYTES - 4 * 1024;

// ─────────────────────────────────────────────────────────────────────────────
//   The settings block — 10 closed fields
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
//   The bands
// ─────────────────────────────────────────────────────────────────────────────
//
// A band rather than a number, because "this church has 847 songs" is close to
// an identifier for a congregation somebody knows, while a band answers every
// question a product decision can actually ask.
//
// **The three vocabularies below are copied by hand from E4's DEPLOYED
// validator** (`sunday-telemetry/src/schema/sundaystage.ts`:
// `STAGE_DISPLAY_BUCKETS`, `STAGE_LIBRARY_BUCKETS`, `STAGE_THEME_BUCKETS`).
// They are written out here rather than derived from anything, for the same
// reason they are written out there: the wire is a contract between two
// repositories, and a rename refactor on either side must not be able to change
// it silently. A band this client invents but the Worker does not know is a 400,
// and a 400 is dropped PERMANENTLY without retry — the whole payload, on every
// machine, for as long as the disagreement lasts.
//
// The library and theme bands are HALF-OPEN and LOWER-BOUND INCLUSIVE: exactly
// 200 songs is `200_1000`, exactly 5 themes is `5_20`.

/// How many displays this machine has. Nobody has two hundred screens, and the
/// difference between one and two is the difference between "the operator is
/// looking at the projector output" and a real multi-display rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/DisplayBucket.ts")]
#[serde(rename_all = "snake_case")]
pub enum DisplayBucket {
    None,
    One,
    Two,
    Three,
    FourPlus,
}

impl DisplayBucket {
    /// The band a count falls in. A negative count (impossible from a `COUNT(*)`,
    /// but the type allows it) reads as none rather than panicking.
    pub fn of(count: i64) -> Self {
        match count {
            i64::MIN..=0 => Self::None,
            1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            _ => Self::FourPlus,
        }
    }

    /// The wire string — `STAGE_DISPLAY_BUCKETS`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::One => "one",
            Self::Two => "two",
            Self::Three => "three",
            Self::FourPlus => "four_plus",
        }
    }
}

/// How big the song library is.
///
/// Coarse on purpose: library size is a decent proxy for how long a congregation
/// has used the app, so the bands are wide enough that no single church sits
/// alone in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/LibraryBucket.ts")]
pub enum LibraryBucket {
    #[serde(rename = "under_50")]
    Under50,
    #[serde(rename = "50_200")]
    From50,
    #[serde(rename = "200_1000")]
    From200,
    #[serde(rename = "1000_5000")]
    From1000,
    #[serde(rename = "over_5000")]
    Over5000,
}

impl LibraryBucket {
    pub fn of(count: i64) -> Self {
        match count {
            i64::MIN..=49 => Self::Under50,
            50..=199 => Self::From50,
            200..=999 => Self::From200,
            1_000..=4_999 => Self::From1000,
            _ => Self::Over5000,
        }
    }

    /// The wire string — `STAGE_LIBRARY_BUCKETS`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Under50 => "under_50",
            Self::From50 => "50_200",
            Self::From200 => "200_1000",
            Self::From1000 => "1000_5000",
            Self::Over5000 => "over_5000",
        }
    }
}

/// How many themes the operator made themselves. Same half-open convention, on a
/// range where the interesting question is "does anyone customise at all".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ThemeBucket.ts")]
pub enum ThemeBucket {
    #[serde(rename = "under_5")]
    Under5,
    #[serde(rename = "5_20")]
    From5,
    #[serde(rename = "20_50")]
    From20,
    #[serde(rename = "over_50")]
    Over50,
}

impl ThemeBucket {
    pub fn of(count: i64) -> Self {
        match count {
            i64::MIN..=4 => Self::Under5,
            5..=19 => Self::From5,
            20..=49 => Self::From20,
            _ => Self::Over50,
        }
    }

    /// The wire string — `STAGE_THEME_BUCKETS`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Under5 => "under_5",
            Self::From5 => "5_20",
            Self::From20 => "20_50",
            Self::Over50 => "over_50",
        }
    }
}

/// The technical settings block: ten closed enum/bool/band fields, and nothing
/// that could carry a name.
///
/// Every field is gathered from real app state by [`WireSettings::gather`] —
/// see that function for exactly which file or table each one reads, because
/// "honest" is a property of the reads, not of the field names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/WireSettings.ts")]
#[serde(rename_all = "camelCase")]
pub struct WireSettings {
    /// Displays actually driven by SundayStage (role != off).
    #[ts(type = "number")]
    pub output_count: u32,
    /// How many displays are connected, as a band.
    pub display_count_bucket: DisplayBucket,
    /// A display is assigned the stage-facing role.
    pub stage_display_enabled: bool,
    /// The companion phone transport is configured.
    pub companion_enabled: bool,
    /// The outputs run as in-process windows rather than isolated processes —
    /// i.e. the crash-isolation fallback is in effect.
    pub fallback_in_process: bool,
    pub library_songs_bucket: LibraryBucket,
    pub theme_count_bucket: ThemeBucket,
    /// An Anthropic key is configured. NEVER the key, and never a prefix of it.
    pub ai_configured: bool,
    /// Cloud sync is configured.
    pub sync_enabled: bool,
    /// Which update ring this install follows.
    pub update_ring: crate::services::update_channel::UpdateChannel,
}

impl Default for WireSettings {
    /// What a machine with nothing configured reports. Used by the preview on a
    /// fresh install and as the fallback when a settings file cannot be read —
    /// an unreadable config must not fail a payload build.
    fn default() -> Self {
        Self {
            output_count: 0,
            display_count_bucket: DisplayBucket::None,
            stage_display_enabled: false,
            companion_enabled: false,
            fallback_in_process: false,
            library_songs_bucket: LibraryBucket::Under50,
            theme_count_bucket: ThemeBucket::Under5,
            ai_configured: false,
            sync_enabled: false,
            update_ring: crate::services::update_channel::UpdateChannel::Stable,
        }
    }
}

impl WireSettings {
    /// Read the ten fields from the machine's actual state.
    ///
    /// Honest-by-construction notes, one per field, because the difference
    /// between telemetry that is trusted and telemetry that is not is whether
    /// each number is the number the app actually behaves by:
    ///
    /// * `output_count` / `display_count_bucket` / `stage_display_enabled` /
    ///   `fallback_in_process` — `output_config.json`, the same file
    ///   `commands::output` reads before it opens anything. `display_count` is
    ///   the number of ASSIGNMENTS, which `display::reconcile` keeps at one per
    ///   connected monitor: reading the saved reconciliation costs nothing and
    ///   asks the OS nothing, so a payload build can never touch a window
    ///   server.
    /// * `library_songs_bucket` / `theme_count_bucket` — `COUNT(*)` through the
    ///   repositories, songs excluding soft-deleted rows and themes counting the
    ///   operator's OWN themes (the built-ins are identical on every install, so
    ///   including them would add a constant and hide the signal).
    /// * `ai_configured` — the keychain MIRROR plus the environment, never the
    ///   keychain itself. See [`super::client::StateKey::AiKeyPresent`]: reading
    ///   a keychain item from an automatic path can raise a BLOCKING GUI
    ///   authorisation dialog on macOS, which this codebase already refuses to
    ///   do on the go-live path (`services::ai::keystore::resolve_noninteractive`).
    /// * `sync_enabled` — `commands::sync::CLOUD_ENABLED`, the same constant the
    ///   sync indicator reports to the operator. It is `false` in every shipped
    ///   build today; when Phase 9 wires a backend, both move together.
    /// * `companion_enabled` — `RealtimeTransport::local_only().is_configured()`,
    ///   the same transport `live_start` builds. Also `false` today, for the
    ///   same reason and from the same source rather than from a literal here.
    /// * `update_ring` — `services::update_channel::load`, the file the updater
    ///   itself resolves its endpoint from.
    pub async fn gather(pool: &SqlitePool, data_dir: &Path) -> Self {
        let cfg = crate::commands::output::saved_config(data_dir);
        let repo = TelemetryRepo::new(pool);

        let songs = repo.song_count().await.unwrap_or(0);
        let themes = repo.custom_theme_count().await.unwrap_or(0);
        let ai_mirror = super::client::ai_key_mirror(pool).await.unwrap_or(false);

        let driven = cfg
            .assignments
            .iter()
            .filter(|a| a.role != crate::services::display::DisplayRole::Off)
            .count();

        Self {
            // Clamped to the range E4's validator accepts (`0..=64`). A rig with
            // sixty-five driven outputs does not exist, but a payload rejected
            // for a number nobody will ever read would be dropped permanently —
            // so the impossible case degrades to "lots" instead of to silence.
            output_count: driven.min(64) as u32,
            display_count_bucket: DisplayBucket::of(cfg.assignments.len() as i64),
            stage_display_enabled: cfg
                .assignments
                .iter()
                .any(|a| a.role == crate::services::display::DisplayRole::StageDisplay),
            companion_enabled:
                crate::services::companion::transport::RealtimeTransport::local_only()
                    .is_configured(),
            fallback_in_process: !cfg.process_isolation,
            library_songs_bucket: LibraryBucket::of(songs),
            theme_count_bucket: ThemeBucket::of(themes),
            ai_configured: ai_mirror || crate::services::ai::keystore::has_env_key(),
            sync_enabled: crate::commands::sync::CLOUD_ENABLED,
            update_ring: crate::services::update_channel::load(data_dir),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The record collections
// ─────────────────────────────────────────────────────────────────────────────

/// One counter's DELTA since the last report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CounterReport.ts")]
#[serde(rename_all = "camelCase")]
pub struct CounterReport {
    /// Serialises AS the dotted wire string — see [`CounterName`]'s serde
    /// renames.
    pub name: CounterName,
    #[ts(type = "number")]
    pub value: i64,
}

/// One finished live session's health, as it goes on the wire.
///
/// [`QualityRow`] minus `id` and `dedupe_key`: both are local bookkeeping, and
/// the dedupe key in particular is derived from a session's start time and cue
/// count, which is more than the endpoint needs to know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/QualityReport.ts")]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    #[ts(type = "number")]
    pub at: i64,
    #[ts(type = "number")]
    pub duration_sec: i64,
    #[ts(type = "number")]
    pub cue_count: i64,
    #[ts(type = "number")]
    pub output_child_restarts: i64,
    #[ts(type = "number")]
    pub connect_timeouts: i64,
    #[ts(type = "number")]
    pub watchdog_holds: i64,
    #[ts(type = "number")]
    pub dispatch_errors: i64,
    #[ts(type = "number")]
    pub companion_failures: i64,
    pub fallback_used: bool,
    pub stale_child_reaped: bool,
    pub abnormal_end: bool,
    pub recovered: bool,
    #[ts(type = "number | null")]
    pub cue_latency_p95_ms: Option<u32>,
    pub verdict: QualityVerdict,
    pub reasons: Vec<QualityReason>,
}

impl From<&QualityRow> for QualityReport {
    fn from(r: &QualityRow) -> Self {
        Self {
            at: r.at,
            duration_sec: r.duration_sec,
            cue_count: r.cue_count,
            output_child_restarts: r.output_child_restarts,
            connect_timeouts: r.connect_timeouts,
            watchdog_holds: r.watchdog_holds,
            dispatch_errors: r.dispatch_errors,
            companion_failures: r.companion_failures,
            fallback_used: r.fallback_used,
            stale_child_reaped: r.stale_child_reaped,
            abnormal_end: r.abnormal_end,
            recovered: r.recovered,
            cue_latency_p95_ms: r.cue_latency_p95_ms,
            verdict: r.verdict,
            reasons: r.reasons.clone(),
        }
    }
}

/// One crash record, as it goes on the wire.
///
/// The E3 ring already writes records in this shape — scrubbed, capped at
/// exactly the wire limits — so this is a projection that drops `schema` and
/// changes NO string. There is deliberately no second truncation pass that could
/// disagree with the one that wrote the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/CrashReport.ts")]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub kind: CrashKind,
    #[ts(type = "number")]
    pub at: i64,
    pub app_version: String,
    pub os: TelemetryOs,
    pub message: String,
    pub location: Option<String>,
    pub task: Option<String>,
    pub backtrace_present: bool,
}

impl From<&crash_ring::CrashEntry> for CrashReport {
    fn from(e: &crash_ring::CrashEntry) -> Self {
        Self {
            kind: e.kind,
            at: e.at,
            // Capped here as well as at write time: a record on disk was written
            // by SOME version of this app, and a future one might raise the cap.
            app_version: sanitize_version(&e.app_version),
            os: e.os,
            message: e.message.clone(),
            location: e.location.clone(),
            task: e.task.clone(),
            backtrace_present: e.backtrace_present,
        }
    }
}

/// Where the operator was when they wrote a manual problem report. A CLOSED set
/// — the one field of a report that is not free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ReportContext.ts")]
#[serde(rename_all = "lowercase")]
pub enum ReportContext {
    Live,
    Editor,
    Settings,
    Other,
}

impl ReportContext {
    /// The stored (and wire) spelling. One word per value, so the SQLite CHECK
    /// constraint, the serde representation and the endpoint's enum cannot drift
    /// apart into three vocabularies.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Editor => "editor",
            Self::Settings => "settings",
            Self::Other => "other",
        }
    }

    /// Parse a stored spelling. `None` for anything outside the closed set — the
    /// allow-list is the point.
    pub fn from_wire(raw: &str) -> Option<Self> {
        match raw {
            "live" => Some(Self::Live),
            "editor" => Some(Self::Editor),
            "settings" => Some(Self::Settings),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// One manual "report a problem" submission.
///
/// **E6 fills this; E5 always sent an empty list.** The field exists on the
/// wire from day one because `STAGE_OPTIONAL_PAYLOAD_KEYS` is empty by design
/// (the programme's Nøkkeldesign): the Worker requires every top-level key, so a
/// client that omitted `reports` until E6 would be rejected with a 400 — dropped
/// permanently, without retry, exactly the silent loss law 3 exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/ProblemReport.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProblemReport {
    #[ts(type = "number")]
    pub at: i64,
    pub context: ReportContext,
    /// Free text, scrubbed and capped at [`super::MESSAGE_MAX_CHARS`].
    pub message: String,
    /// The scrubbed log tail, capped at [`LOG_TAIL_MAX_CHARS`].
    pub log_tail: String,
}

/// Cap on a report's log tail. The most dangerous free-text line in the whole
/// system, hence three defences: no content in logs at all (E3's audit), a
/// client-side scrub when written AND when read, and the Worker's
/// `ABSOLUTE_PATH_RE` as the last line.
pub const LOG_TAIL_MAX_CHARS: usize = 4_000;

impl ProblemReport {
    /// Build a report from raw operator input, scrubbed and capped exactly once.
    ///
    /// E6's dialog previews the result of THIS function, so the bytes on screen
    /// are the bytes on the wire.
    pub fn new(
        at: i64,
        context: ReportContext,
        message: &str,
        log_tail: &str,
        home: Option<&str>,
    ) -> Self {
        use crate::telemetry::scrub;
        Self {
            at,
            context,
            message: scrub::free_text(message, home, super::MESSAGE_MAX_CHARS),
            log_tail: scrub::free_text(log_tail, home, LOG_TAIL_MAX_CHARS),
        }
    }
}

/// A [`ProblemReport`] as it is held between "the operator pressed send" and
/// "these bytes were actually included in a payload".
///
/// The two local facts the wire never sees: the row id, and WHICH consent this
/// report travels on. `ephemeral` is decided once, at submit time, from the
/// standing consent state — a later grant does not sweep an ephemeral report
/// into the durable stream, and a later revoke does not withdraw the one-shot
/// consent that was given for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReport {
    pub id: String,
    pub ephemeral: bool,
    pub report: ProblemReport,
}

/// The payload one ephemeral report travels in, under a one-shot id.
///
/// Everything an ephemeral send does NOT carry is the interesting part:
/// counters, crashes, quality rows and the real settings block are all left at
/// their empty/default values. Pressing send on a report is consent for that
/// report — not for the machine's feature usage, not for its configuration and
/// not for its crash history. The envelope keeps only what makes a bug report
/// actionable at all (app version, OS, arch, UI language), and the dialog's
/// preview shows the operator these exact fields before they press anything.
///
/// `install_id` is generated by the CALLER, per send, and stored nowhere.
pub fn build_ephemeral_report_payload(
    install_id: &str,
    consent_version: u32,
    app_version: &str,
    language: Option<&str>,
    now_ms: i64,
    report: ProblemReport,
) -> StagePayload {
    let mut payload = StagePayload::new(install_id, consent_version, app_version, now_ms);
    payload.language = sanitize_language(language);
    payload.reports.push(report);
    payload
}

// ─────────────────────────────────────────────────────────────────────────────
//   The payload
// ─────────────────────────────────────────────────────────────────────────────

/// The complete payload — the only thing that ever leaves the machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/StagePayload.ts")]
#[serde(rename_all = "camelCase")]
pub struct StagePayload {
    /// [`PAYLOAD_SCHEMA`] at build time.
    pub schema: u32,
    /// The consent scope this payload was collected under, so the endpoint can
    /// prove per row which question the operator actually answered.
    pub consent_version: u32,
    /// The random, machine-local install id. Never derived from anything.
    pub install_id: String,
    /// Unix ms (UTC) **the payload was built**.
    ///
    /// Divergence note: the E5 brief glossed this as a compile-time build stamp.
    /// It is not, and must not be: the programme pins the envelope as "som Rec",
    /// where `builtAt` is the payload's own build time, and a constant baked in
    /// at compile time would give every payload from one release an identical
    /// timestamp — useless for ordering and indistinguishable from a stuck
    /// clock. The app's build identity is already carried, precisely, by
    /// [`Self::app_version`].
    #[ts(type = "number")]
    pub built_at: i64,
    /// `CARGO_PKG_VERSION` of the running build.
    pub app_version: String,
    pub os: TelemetryOs,
    pub arch: TelemetryArch,
    /// UI language, PRIMARY SUBTAG ONLY (`nb-NO` → `nb`). `None` when the app
    /// has not recorded a choice. The region subtag is deliberately discarded:
    /// language answers a real product question (which translations get used),
    /// region narrows an anonymous install towards a place.
    pub language: Option<String>,
    pub settings: WireSettings,
    pub counters: Vec<CounterReport>,
    pub crashes: Vec<CrashReport>,
    pub quality: Vec<QualityReport>,
    /// Always empty until E6 — see [`ProblemReport`] for why the key is here
    /// anyway.
    pub reports: Vec<ProblemReport>,
}

impl StagePayload {
    /// The header every payload carries, with no records in it.
    pub fn new(install_id: &str, consent_version: u32, app_version: &str, built_at: i64) -> Self {
        Self {
            schema: PAYLOAD_SCHEMA,
            consent_version,
            install_id: sanitize_install_id(install_id),
            built_at,
            app_version: sanitize_version(app_version),
            os: TelemetryOs::current(),
            arch: TelemetryArch::current(),
            language: None,
            settings: WireSettings::default(),
            counters: Vec::new(),
            crashes: Vec::new(),
            quality: Vec::new(),
            reports: Vec::new(),
        }
    }

    /// Whether this payload carries anything worth sending.
    ///
    /// A payload with no crashes, no quality rows, no reports and no non-zero
    /// counter is just a header — sending it would be a ping, and a ping is not
    /// what the operator consented to. The settings block does not count: it
    /// describes the machine, and on its own it is not news.
    pub fn is_empty(&self) -> bool {
        self.crashes.is_empty()
            && self.quality.is_empty()
            && self.reports.is_empty()
            && self.counters.iter().all(|c| c.value == 0)
    }

    /// Trim every collection to its COUNT cap, keeping the NEWEST records.
    pub fn truncate_to_caps(&mut self) {
        keep_last(&mut self.crashes, MAX_CRASHES);
        keep_last(&mut self.quality, MAX_QUALITY);
        keep_last(&mut self.reports, MAX_REPORTS);
    }

    /// The serialised size of this payload in BYTES — what the Worker measures.
    ///
    /// Counting characters is not counting bytes, and the whole reason this
    /// function exists is that the two disagree by up to 4× on exactly the text
    /// a crash message is most likely to contain.
    pub fn body_bytes(&self) -> usize {
        serde_json::to_vec(self)
            .map(|v| v.len())
            .unwrap_or(usize::MAX)
    }

    /// Trim until the SERIALISED BYTES fit under `target`.
    ///
    /// ## Why drop-oldest, and what it costs
    ///
    /// Collections are trimmed from the front, in this order, and the order is
    /// the argument:
    ///
    /// 1. **quality** — free. A row that does not go in this payload keeps its
    ///    `sent_at` NULL and goes in the next one, so trimming here delays data
    ///    and never loses it.
    /// 2. **reports** — free on the same terms, provided E6 marks a manual
    ///    report as sent only when it was actually included. **E6 must honour
    ///    that**, or this becomes the one place an operator's hand-written words
    ///    are dropped.
    /// 3. **crashes** — the only lossy one, and last for that reason. The crash
    ///    watermark advances past dropped records (it must: leaving it behind
    ///    would rebuild the same oversized payload every beat, trim it the same
    ///    way, and loop forever). So a dropped crash record is gone from the
    ///    wire, and the caller LOGS it — bounded, documented, never silent.
    ///
    /// Counters are never dropped: nineteen small entries, and each is a delta
    /// whose events cannot be recovered from anything else.
    ///
    /// Re-measuring after each drop rather than estimating is deliberate. The
    /// estimate would have to model JSON escaping and UTF-8 width — the two
    /// things that caused this problem — and this runs off the live path at most
    /// once a minute.
    pub fn fit_to_body_cap(&mut self, target: usize) -> FitReport {
        let mut report = FitReport {
            bytes: self.body_bytes(),
            ..Default::default()
        };
        while report.bytes > target {
            if !self.quality.is_empty() {
                self.quality.remove(0);
                report.quality_dropped += 1;
            } else if !self.reports.is_empty() {
                self.reports.remove(0);
                report.reports_dropped += 1;
            } else if !self.crashes.is_empty() {
                self.crashes.remove(0);
                report.crashes_dropped += 1;
            } else {
                // A header plus a settings block is a few hundred bytes; there
                // is nothing left to drop and no way this is reachable. Stop
                // rather than spin — an oversized header is a bug to see in the
                // logs, not a loop to hang the pump on.
                report.over_cap = true;
                break;
            }
            report.bytes = self.body_bytes();
        }
        report
    }
}

/// What [`StagePayload::fit_to_body_cap`] had to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FitReport {
    /// The final serialised size in bytes.
    pub bytes: usize,
    pub quality_dropped: usize,
    pub reports_dropped: usize,
    /// Crash records dropped. The lossy one — worth a log line.
    pub crashes_dropped: usize,
    /// Nothing was left to drop and it still did not fit. Not reachable with the
    /// current caps; recorded rather than asserted.
    pub over_cap: bool,
}

impl FitReport {
    /// Whether anything was dropped at all.
    pub fn trimmed(&self) -> bool {
        self.quality_dropped + self.reports_dropped + self.crashes_dropped > 0
    }
}

/// Keep at most `cap` items, dropping from the FRONT (the oldest).
fn keep_last<T>(v: &mut Vec<T>, cap: usize) {
    if v.len() > cap {
        v.drain(..v.len() - cap);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   Wire sanitizers
// ─────────────────────────────────────────────────────────────────────────────

/// A UUID-shaped install id, lowercased. Anything else becomes
/// [`NIL_INSTALL_ID`] — the wire cannot carry a hand-edited identifier.
pub fn sanitize_install_id(raw: &str) -> String {
    let ok = raw.len() == 36
        && raw.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
    if ok {
        raw.to_ascii_lowercase()
    } else {
        NIL_INSTALL_ID.to_string()
    }
}

/// A version string: the LEADING run of version-shaped characters, capped at
/// [`VERSION_MAX_CHARS`].
///
/// Takes a leading run rather than FILTERING, and the difference matters: a
/// filter turns `"0.5.0 (built by Kari)"` into `"0.5.0builtbyKari"` — the
/// separators vanish and the name survives, which is the opposite of the intent.
/// Stopping at the first character a version cannot contain yields `"0.5.0"`.
pub fn sanitize_version(raw: &str) -> String {
    raw.trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        .take(VERSION_MAX_CHARS)
        .collect()
}

/// The PRIMARY language subtag only, lowercased: `"nb-NO"` → `"nb"`. `None` when
/// unset or not alphabetic.
pub fn sanitize_language(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    let primary: String = raw
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(LANGUAGE_MAX_CHARS)
        .collect::<String>()
        .to_ascii_lowercase();
    if primary.is_empty() {
        None
    } else {
        Some(primary)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The builder
// ─────────────────────────────────────────────────────────────────────────────

/// Everything a build needs that is not in the database.
pub struct GatherContext<'a> {
    /// `<app-local-data>` — the crash ring and the settings files live under it.
    pub app_data_dir: &'a Path,
    /// The running build's version.
    pub app_version: &'a str,
    /// The operator's home directory, for the free-text scrubbers.
    pub home: Option<&'a str>,
    /// Unix ms (UTC) now.
    pub now_ms: i64,
    /// The consent scope this payload is collected under.
    pub consent_version: u32,
}

/// What a successful enqueue should commit, so the same data is not reported
/// twice. Produced by [`build`], applied by
/// [`crate::db::repositories::TelemetryRepo::commit_marks`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DrainMarks {
    /// The new crash watermark: the newest `at` this payload carries, or the old
    /// watermark when it carries no crash.
    pub crash_at: i64,
    /// Quality rows to stamp `sent_at`.
    pub quality_ids: Vec<String>,
    /// Manual reports to stamp `sent_at` — **only the ones the finished payload
    /// actually carries**. See [`StagePayload::fit_to_body_cap`]: reports are
    /// trimmed as "free to defer", and that is only true if a deferred one is
    /// still owed afterwards.
    pub report_ids: Vec<String>,
    /// Counters and the TOTAL each was reported up to, to store as `sent_value`.
    pub counters: Vec<(CounterName, i64)>,
}

/// Build the payload for everything newer than the watermarks.
///
/// Pure with respect to the database: it READS, it never writes. The caller
/// decides whether the result is worth queueing ([`StagePayload::is_empty`]) and
/// only then commits the marks.
pub async fn build(
    pool: &SqlitePool,
    ctx: &GatherContext<'_>,
    since_crash_at: i64,
    install_id: Option<&str>,
) -> AppResult<(StagePayload, DrainMarks)> {
    build_to(pool, ctx, since_crash_at, install_id, BODY_TARGET_BYTES).await
}

/// [`build`] with an explicit byte target.
///
/// The target is a parameter for one reason: the trimming path and the
/// mark-lockstep that depends on it must be provable NOW. E5 never fills
/// `reports`, so a payload this stage can actually produce stays around 40 kB
/// even in the four-byte worst case, and an end-to-end test of the trim would
/// have to wait for E6 to become reachable. A test that cannot run is not a
/// test — so the tests drive a small target through the real code path.
pub async fn build_to(
    pool: &SqlitePool,
    ctx: &GatherContext<'_>,
    since_crash_at: i64,
    install_id: Option<&str>,
    body_target: usize,
) -> AppResult<(StagePayload, DrainMarks)> {
    let repo = TelemetryRepo::new(pool);
    let mut marks = DrainMarks {
        crash_at: since_crash_at,
        ..Default::default()
    };

    let mut payload = StagePayload::new(
        install_id.unwrap_or(NIL_INSTALL_ID),
        ctx.consent_version,
        ctx.app_version,
        ctx.now_ms,
    );
    payload.language = sanitize_language(super::client::language(pool).await?.as_deref());
    payload.settings = WireSettings::gather(pool, ctx.app_data_dir).await;

    // ── Counters: the DELTA each has moved since it was last reported ────────
    for (name, total, sent) in repo.counter_watermarks().await? {
        let delta = total - sent;
        if delta > 0 {
            payload.counters.push(CounterReport { name, value: delta });
            marks.counters.push((name, total));
        }
    }

    // ── Quality: rows never marked sent ──────────────────────────────────────
    //
    // `quality_ids` is kept in LOCKSTEP with `payload.quality` — same order,
    // same length — so that every trim below can be applied to both. A mark that
    // claimed a row the payload does not carry would lose that row silently,
    // which is precisely the failure the byte cap introduced.
    let mut rows = repo.unsent_quality(MAX_QUALITY as i64 * 2).await?;
    rows.sort_by_key(|r| r.at);
    let mut quality_ids: Vec<String> = Vec::with_capacity(rows.len());
    for row in &rows {
        quality_ids.push(row.id.clone());
        payload.quality.push(QualityReport::from(row));
    }

    // ── Reports: what the operator wrote, oldest first ───────────────────────
    //
    // Same lockstep discipline as quality, for a stronger reason: these are
    // hand-written words, and the one thing the byte cap must never do is eat
    // them silently. `report_ids` is trimmed by exactly what the caps removed,
    // so a deferred report keeps `sent_at` NULL and the settings panel can still
    // say it is waiting.
    let stored_reports = repo.unsent_reports(false, MAX_REPORTS as i64 * 2).await?;
    let mut report_ids: Vec<String> = Vec::with_capacity(stored_reports.len());
    for stored in stored_reports {
        report_ids.push(stored.id);
        payload.reports.push(stored.report);
    }

    // ── Crashes: the file ring, above the watermark ──────────────────────────
    let dir = crash_ring::crash_dir(ctx.app_data_dir);
    for entry in crash_ring::read_entries(&dir) {
        if entry.at <= since_crash_at {
            continue;
        }
        // Advanced for every record READ, including any the byte cap drops
        // below. Leaving the watermark behind a dropped record would rebuild the
        // same oversized payload on the next beat, trim it identically, and
        // loop — a permanent stall that also never sends the newer records.
        marks.crash_at = marks.crash_at.max(entry.at);
        payload.crashes.push(CrashReport::from(&entry));
    }
    payload.crashes.sort_by_key(|c| c.at);

    // ── The two caps: count first, then BYTES ────────────────────────────────
    let quality_before = payload.quality.len();
    let reports_before = payload.reports.len();
    payload.truncate_to_caps();
    let fit = payload.fit_to_body_cap(body_target);

    // Whatever the two caps removed from the front of `quality`, remove from the
    // front of the id list too, so the marks describe what actually shipped.
    let removed = quality_before - payload.quality.len();
    quality_ids.drain(..removed);
    marks.quality_ids = quality_ids;
    // The same, for the operator's own words. Both caps take from the FRONT
    // (`truncate_to_caps` keeps the newest, `fit_to_body_cap` removes index 0),
    // so the surviving ids are the tail of the list.
    let reports_removed = reports_before - payload.reports.len();
    report_ids.drain(..reports_removed);
    marks.report_ids = report_ids;

    if fit.crashes_dropped > 0 {
        // The one lossy trim, and therefore the one that gets said out loud.
        tracing::warn!(
            dropped = fit.crashes_dropped,
            bytes = fit.bytes,
            "telemetry: a payload exceeded the {BODY_MAX_BYTES}-byte body cap; the oldest crash \
             records were dropped to fit. They will not be reported — a 413 would have discarded \
             the whole payload just as permanently, and without this line."
        );
    } else if fit.trimmed() {
        tracing::debug!(
            quality = fit.quality_dropped,
            reports = fit.reports_dropped,
            bytes = fit.bytes,
            "telemetry: a payload was trimmed to fit the body cap; the trimmed records stay \
             unsent (their marks were trimmed with them) and go in the next one"
        );
    }
    Ok((payload, marks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::telemetry::crash_ring::CrashEntry;
    use crate::telemetry::quality::{QualityReason, QualityVerdict};
    use crate::telemetry::RECORD_SCHEMA;

    fn ctx<'a>(dir: &'a Path, now: i64) -> GatherContext<'a> {
        GatherContext {
            app_data_dir: dir,
            app_version: "0.5.0",
            home: Some("/Users/ola"),
            now_ms: now,
            consent_version: 1,
        }
    }

    fn quality_row(id: &str, at: i64) -> QualityRow {
        QualityRow {
            id: id.to_string(),
            dedupe_key: None,
            at,
            duration_sec: 3_600,
            cue_count: 42,
            output_child_restarts: 1,
            connect_timeouts: 0,
            watchdog_holds: 1,
            dispatch_errors: 0,
            companion_failures: 0,
            fallback_used: false,
            stale_child_reaped: false,
            abnormal_end: false,
            recovered: false,
            cue_latency_p95_ms: Some(18),
            verdict: QualityVerdict::Warn,
            reasons: vec![QualityReason::OutputRestarted, QualityReason::HoldLastFrame],
        }
    }

    fn write_crash(dir: &Path, at: i64, message: &str) {
        let ring = crash_ring::crash_dir(dir);
        std::fs::create_dir_all(&ring).expect("ring dir");
        let entry = CrashEntry {
            schema: RECORD_SCHEMA,
            kind: CrashKind::Panic,
            at,
            app_version: "0.4.0".into(),
            os: TelemetryOs::Macos,
            message: message.to_string(),
            location: Some("src/live.rs:12:3".into()),
            task: None,
            backtrace_present: true,
        };
        std::fs::write(
            ring.join(format!("crash-{at:015}-0000.json")),
            serde_json::to_vec(&entry).expect("serialise"),
        )
        .expect("write");
    }

    // ── Buckets ──────────────────────────────────────────────────────────────

    #[test]
    fn the_bands_are_half_open_and_lower_bound_inclusive() {
        // The convention E4's validator documents: exactly 200 songs is
        // `200_1000`, exactly 5 themes is `5_20`. An off-by-one here is not a
        // rejection — it is a quietly wrong answer in an aggregate nobody can
        // audit afterwards, which is worse.
        assert_eq!(LibraryBucket::of(0), LibraryBucket::Under50);
        assert_eq!(
            LibraryBucket::of(-7),
            LibraryBucket::Under50,
            "never panics"
        );
        assert_eq!(LibraryBucket::of(49), LibraryBucket::Under50);
        assert_eq!(LibraryBucket::of(50), LibraryBucket::From50);
        assert_eq!(LibraryBucket::of(199), LibraryBucket::From50);
        assert_eq!(LibraryBucket::of(200), LibraryBucket::From200);
        assert_eq!(LibraryBucket::of(999), LibraryBucket::From200);
        assert_eq!(LibraryBucket::of(1_000), LibraryBucket::From1000);
        assert_eq!(LibraryBucket::of(4_999), LibraryBucket::From1000);
        assert_eq!(LibraryBucket::of(5_000), LibraryBucket::Over5000);
        assert_eq!(LibraryBucket::of(i64::MAX), LibraryBucket::Over5000);

        assert_eq!(ThemeBucket::of(0), ThemeBucket::Under5);
        assert_eq!(ThemeBucket::of(-7), ThemeBucket::Under5, "never panics");
        assert_eq!(ThemeBucket::of(4), ThemeBucket::Under5);
        assert_eq!(ThemeBucket::of(5), ThemeBucket::From5);
        assert_eq!(ThemeBucket::of(19), ThemeBucket::From5);
        assert_eq!(ThemeBucket::of(20), ThemeBucket::From20);
        assert_eq!(ThemeBucket::of(49), ThemeBucket::From20);
        assert_eq!(ThemeBucket::of(50), ThemeBucket::Over50);
        assert_eq!(ThemeBucket::of(i64::MAX), ThemeBucket::Over50);
    }

    #[test]
    fn display_buckets_stop_counting_at_four() {
        assert_eq!(DisplayBucket::of(0), DisplayBucket::None);
        assert_eq!(DisplayBucket::of(-1), DisplayBucket::None, "never panics");
        assert_eq!(DisplayBucket::of(1), DisplayBucket::One);
        assert_eq!(DisplayBucket::of(2), DisplayBucket::Two);
        assert_eq!(DisplayBucket::of(3), DisplayBucket::Three);
        assert_eq!(DisplayBucket::of(4), DisplayBucket::FourPlus);
        assert_eq!(DisplayBucket::of(40), DisplayBucket::FourPlus);
    }

    /// The band vocabularies, copied from E4's DEPLOYED validator
    /// (`sunday-telemetry/src/schema/sundaystage.ts`).
    ///
    /// This is the cross-repo pin. A band this client can produce but the
    /// Worker's `c.enum` does not know is a 400 — dropped permanently, without
    /// retry, on every machine, for as long as the disagreement lasts. The
    /// lists are duplicated by hand on both sides precisely so a refactor on
    /// either side fails a test instead of the field.
    #[test]
    fn every_band_is_a_value_the_deployed_validator_accepts() {
        const STAGE_DISPLAY_BUCKETS: [&str; 5] = ["none", "one", "two", "three", "four_plus"];
        const STAGE_LIBRARY_BUCKETS: [&str; 5] =
            ["under_50", "50_200", "200_1000", "1000_5000", "over_5000"];
        const STAGE_THEME_BUCKETS: [&str; 4] = ["under_5", "5_20", "20_50", "over_50"];
        const STAGE_UPDATE_RINGS: [&str; 2] = ["stable", "beta"];

        let displays = [
            DisplayBucket::None,
            DisplayBucket::One,
            DisplayBucket::Two,
            DisplayBucket::Three,
            DisplayBucket::FourPlus,
        ];
        let libraries = [
            LibraryBucket::Under50,
            LibraryBucket::From50,
            LibraryBucket::From200,
            LibraryBucket::From1000,
            LibraryBucket::Over5000,
        ];
        let themes = [
            ThemeBucket::Under5,
            ThemeBucket::From5,
            ThemeBucket::From20,
            ThemeBucket::Over50,
        ];

        // Every variant serialises to its `as_str`, and every one of those is in
        // the Worker's list — in the same ORDER, so a reordering that changed a
        // meaning would show up too.
        let got: Vec<&str> = displays.iter().map(|b| b.as_str()).collect();
        assert_eq!(got, STAGE_DISPLAY_BUCKETS);
        let got: Vec<&str> = libraries.iter().map(|b| b.as_str()).collect();
        assert_eq!(got, STAGE_LIBRARY_BUCKETS);
        let got: Vec<&str> = themes.iter().map(|b| b.as_str()).collect();
        assert_eq!(got, STAGE_THEME_BUCKETS);

        for b in displays {
            assert_eq!(
                serde_json::to_string(&b).expect("serialises"),
                format!("\"{}\"", b.as_str())
            );
        }
        for b in libraries {
            assert_eq!(
                serde_json::to_string(&b).expect("serialises"),
                format!("\"{}\"", b.as_str())
            );
        }
        for b in themes {
            assert_eq!(
                serde_json::to_string(&b).expect("serialises"),
                format!("\"{}\"", b.as_str())
            );
        }
        // …and the update ring, whose wire strings come from `UpdateChannel`.
        for (c, expected) in [
            (
                crate::services::update_channel::UpdateChannel::Stable,
                "stable",
            ),
            (crate::services::update_channel::UpdateChannel::Beta, "beta"),
        ] {
            assert_eq!(
                serde_json::to_value(c).expect("serialises"),
                serde_json::json!(expected)
            );
            assert!(STAGE_UPDATE_RINGS.contains(&expected));
        }
    }

    // ── Sanitizers ───────────────────────────────────────────────────────────

    #[test]
    fn an_install_id_must_be_uuid_shaped_or_it_becomes_nil() {
        let real = uuid::Uuid::now_v7().to_string();
        assert_eq!(sanitize_install_id(&real), real);
        assert_eq!(
            sanitize_install_id("018F3A2B-7C4D-7E1F-9A2B-3C4D5E6F7A8B"),
            "018f3a2b-7c4d-7e1f-9a2b-3c4d5e6f7a8b",
            "lowercased, so one install has one spelling"
        );
        for bad in [
            "",
            "nordstrand-kirke",
            "018f3a2b7c4d7e1f9a2b3c4d5e6f7a8b",
            "018f3a2b-7c4d-7e1f-9a2b-3c4d5e6f7a8",
            "018f3a2b-7c4d-7e1f-9a2b-3c4d5e6f7a8bb",
            "zzzzzzzz-7c4d-7e1f-9a2b-3c4d5e6f7a8b",
        ] {
            assert_eq!(sanitize_install_id(bad), NIL_INSTALL_ID, "{bad:?}");
        }
    }

    #[test]
    fn a_version_stops_at_the_first_thing_a_version_cannot_contain() {
        assert_eq!(sanitize_version("0.5.0"), "0.5.0");
        assert_eq!(sanitize_version("  0.6.0-beta.1 "), "0.6.0-beta.1");
        assert_eq!(
            sanitize_version("0.5.0 (built by Kari)"),
            "0.5.0",
            "a name after the version must not survive as a run-on word"
        );
        assert_eq!(sanitize_version("").len(), 0);
        assert!(sanitize_version(&"9".repeat(100)).chars().count() <= VERSION_MAX_CHARS);
    }

    #[test]
    fn a_language_keeps_only_its_primary_subtag() {
        assert_eq!(sanitize_language(Some("nb-NO")), Some("nb".into()));
        assert_eq!(sanitize_language(Some("pt_BR")), Some("pt".into()));
        assert_eq!(sanitize_language(Some("NO")), Some("no".into()));
        assert_eq!(sanitize_language(Some("  en  ")), Some("en".into()));
        assert_eq!(sanitize_language(None), None);
        assert_eq!(sanitize_language(Some("")), None);
        assert_eq!(sanitize_language(Some("123")), None);
    }

    // ── Caps + emptiness ─────────────────────────────────────────────────────

    #[test]
    fn the_caps_keep_the_newest_records() {
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);
        for at in 0..(MAX_CRASHES as i64 + 5) {
            p.crashes.push(CrashReport {
                kind: CrashKind::Panic,
                at,
                app_version: "0.5.0".into(),
                os: TelemetryOs::Macos,
                message: format!("crash {at}"),
                location: None,
                task: None,
                backtrace_present: false,
            });
        }
        p.truncate_to_caps();
        assert_eq!(p.crashes.len(), MAX_CRASHES);
        assert_eq!(
            p.crashes[0].at, 5,
            "the OLDEST go: a full ring's newest records describe today"
        );
    }

    #[test]
    fn a_header_only_payload_is_empty_and_a_zero_counter_does_not_rescue_it() {
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);
        assert!(p.is_empty());
        p.counters.push(CounterReport {
            name: CounterName::LiveCueDispatched,
            value: 0,
        });
        assert!(p.is_empty(), "a zero counter is a header wearing a list");
        p.counters[0].value = 1;
        assert!(!p.is_empty());
    }

    #[test]
    fn every_record_collection_counts_towards_is_empty() {
        // A collection missing from `is_empty` would be printed on screen by E6's
        // preview underneath the words "nothing to send right now".
        let base = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);

        let mut with_quality = base.clone();
        with_quality
            .quality
            .push(QualityReport::from(&quality_row("q", 1)));
        assert!(!with_quality.is_empty());

        let mut with_crash = base.clone();
        with_crash.crashes.push(CrashReport {
            kind: CrashKind::Panic,
            at: 1,
            app_version: "0.5.0".into(),
            os: TelemetryOs::Macos,
            message: "boom".into(),
            location: None,
            task: None,
            backtrace_present: false,
        });
        assert!(!with_crash.is_empty());

        let mut with_report = base.clone();
        with_report.reports.push(ProblemReport::new(
            1,
            ReportContext::Live,
            "the screen went black",
            "",
            None,
        ));
        assert!(!with_report.is_empty());
    }

    // ── Free text ────────────────────────────────────────────────────────────

    #[test]
    fn report_free_text_is_scrubbed_and_capped_with_the_ellipsis_convention() {
        // The SAME convention E4's Worker validates against: a truncated string
        // is exactly `max + 1` chars, the last one being the ellipsis. A client
        // that cut at `max` and a Worker that expected `max + 1` would 400 every
        // long report — dropped permanently, never retried.
        let long = "a".repeat(super::super::MESSAGE_MAX_CHARS + 50);
        let r = ProblemReport::new(1, ReportContext::Other, &long, "", None);
        assert_eq!(
            r.message.chars().count(),
            super::super::MESSAGE_MAX_CHARS + 1
        );
        assert!(r.message.ends_with('…'));

        let tail = "b".repeat(LOG_TAIL_MAX_CHARS + 10);
        let r = ProblemReport::new(1, ReportContext::Other, "x", &tail, None);
        assert_eq!(r.log_tail.chars().count(), LOG_TAIL_MAX_CHARS + 1);
        assert!(r.log_tail.ends_with('…'));

        // …and a report that fits is not touched at all.
        let r = ProblemReport::new(1, ReportContext::Live, "short", "log", None);
        assert_eq!(r.message, "short");
        assert!(!r.message.ends_with('…'));

        // The operator's home directory never reaches the wire.
        let r = ProblemReport::new(
            1,
            ReportContext::Editor,
            "failed to open /Users/ola/Music/Salmer/Deg være ære.mp3",
            "ERROR /Users/ola/Library/Application Support/no.sundaystage.app/x.db",
            Some("/Users/ola"),
        );
        assert!(!r.message.contains("/Users/ola"), "{}", r.message);
        assert!(!r.log_tail.contains("/Users/ola"), "{}", r.log_tail);
    }

    // ── The builder ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_fresh_machine_builds_an_empty_payload() {
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let (p, marks) = build(&db.pool, &ctx(dir.path(), 1_800_000_000_000), 0, None)
            .await
            .expect("build");
        assert!(p.is_empty());
        assert_eq!(p.install_id, NIL_INSTALL_ID, "no id is minted by a build");
        assert_eq!(p.schema, PAYLOAD_SCHEMA);
        assert_eq!(p.built_at, 1_800_000_000_000);
        assert_eq!(p.app_version, "0.5.0");
        assert_eq!(marks, DrainMarks::default());
        assert!(p.reports.is_empty(), "E5 never fills reports");
    }

    #[tokio::test]
    async fn the_builder_reads_the_real_e3_shapes() {
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[
            (CounterName::LiveCueDispatched, 40),
            (CounterName::ThemeCreated, 2),
        ])
        .await
        .expect("counters");
        repo.insert_quality(&quality_row("q1", 5_000))
            .await
            .expect("quality");
        write_crash(
            dir.path(),
            9_000,
            "called `Option::unwrap()` on a `None` value",
        );

        let (p, marks) = build(&db.pool, &ctx(dir.path(), 10_000), 0, None)
            .await
            .expect("build");

        assert_eq!(p.counters.len(), 2);
        let cues = p
            .counters
            .iter()
            .find(|c| c.name == CounterName::LiveCueDispatched)
            .expect("cue counter");
        assert_eq!(cues.value, 40);
        assert_eq!(p.quality.len(), 1);
        assert_eq!(p.quality[0].at, 5_000);
        assert_eq!(p.quality[0].verdict, QualityVerdict::Warn);
        assert_eq!(p.crashes.len(), 1);
        assert_eq!(p.crashes[0].at, 9_000);
        assert!(p.crashes[0]
            .message
            .starts_with("called `Option::unwrap()`"));
        assert!(!p.is_empty());

        // The marks describe exactly what was drained.
        assert_eq!(marks.crash_at, 9_000);
        assert_eq!(marks.quality_ids, vec!["q1".to_string()]);
        assert!(marks
            .counters
            .contains(&(CounterName::LiveCueDispatched, 40)));

        // …and the quality projection drops the LOCAL bookkeeping.
        let json = serde_json::to_string(&p.quality[0]).expect("serialises");
        assert!(!json.contains("dedupeKey"), "{json}");
        assert!(!json.contains("\"id\""), "{json}");
    }

    #[tokio::test]
    async fn building_twice_without_committing_yields_identical_bytes() {
        // The read-only property the preview rests on: a hundred previews change
        // nothing, and the preview a user reads is the payload that would be
        // queued.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        TelemetryRepo::new(&db.pool)
            .add_counters(&[(CounterName::LiveCueDispatched, 3)])
            .await
            .expect("counters");

        let (a, _) = build(&db.pool, &ctx(dir.path(), 10_000), 0, None)
            .await
            .expect("first");
        let (b, _) = build(&db.pool, &ctx(dir.path(), 10_000), 0, None)
            .await
            .expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("a"),
            serde_json::to_string(&b).expect("b")
        );
    }

    #[tokio::test]
    async fn committing_the_marks_makes_the_next_build_empty() {
        // Idempotence, the property the whole watermark design exists for: a
        // datum is reported once, whatever the drain's schedule turns out to be.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 3)])
            .await
            .expect("counters");
        repo.insert_quality(&quality_row("q1", 5_000))
            .await
            .expect("quality");
        write_crash(dir.path(), 9_000, "boom");

        let (first, marks) = build(&db.pool, &ctx(dir.path(), 10_000), 0, None)
            .await
            .expect("build");
        assert!(!first.is_empty());
        repo.commit_marks(&marks).await.expect("commit");

        let (second, marks2) = build(&db.pool, &ctx(dir.path(), 10_000), marks.crash_at, None)
            .await
            .expect("rebuild");
        assert!(second.is_empty(), "everything was already reported");
        assert!(second.counters.is_empty());
        assert!(second.quality.is_empty());
        assert!(second.crashes.is_empty());
        assert_eq!(marks2.quality_ids.len(), 0);

        // …and a NEW event after the commit is reported, so the watermark did
        // not simply switch everything off.
        repo.add_counters(&[(CounterName::LiveCueDispatched, 2)])
            .await
            .expect("more");
        let (third, _) = build(&db.pool, &ctx(dir.path(), 11_000), marks.crash_at, None)
            .await
            .expect("third");
        assert_eq!(third.counters.len(), 1);
        assert_eq!(
            third.counters[0].value, 2,
            "the DELTA since the last report, not the running total"
        );
    }

    #[tokio::test]
    async fn a_crash_at_the_watermark_is_not_reported_twice() {
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        write_crash(dir.path(), 9_000, "old");
        write_crash(dir.path(), 12_000, "new");

        let (p, marks) = build(&db.pool, &ctx(dir.path(), 20_000), 9_000, None)
            .await
            .expect("build");
        assert_eq!(p.crashes.len(), 1, "the record AT the watermark is done");
        assert_eq!(p.crashes[0].at, 12_000);
        assert_eq!(marks.crash_at, 12_000);
    }

    #[tokio::test]
    async fn the_settings_block_reads_the_files_the_app_behaves_by() {
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");

        // A rig: three monitors, two driven, one of them the stage display, and
        // the crash-isolation fallback in effect.
        let cfg = crate::services::display::OutputConfig {
            assignments: vec![
                crate::services::display::DisplayAssignment {
                    monitor_index: 0,
                    role: crate::services::display::DisplayRole::Off,
                },
                crate::services::display::DisplayAssignment {
                    monitor_index: 1,
                    role: crate::services::display::DisplayRole::MainOutput,
                },
                crate::services::display::DisplayAssignment {
                    monitor_index: 2,
                    role: crate::services::display::DisplayRole::StageDisplay,
                },
            ],
            process_isolation: false,
        };
        std::fs::write(
            dir.path().join("output_config.json"),
            serde_json::to_string(&cfg).expect("cfg"),
        )
        .expect("write");
        crate::services::update_channel::save(
            dir.path(),
            crate::services::update_channel::UpdateChannel::Beta,
        )
        .expect("channel");

        let s = WireSettings::gather(&db.pool, dir.path()).await;
        assert_eq!(s.output_count, 2, "an 'off' display is not an output");
        assert_eq!(s.display_count_bucket, DisplayBucket::Three);
        assert!(s.stage_display_enabled);
        assert!(s.fallback_in_process, "isolation off IS the fallback");
        assert_eq!(
            s.update_ring,
            crate::services::update_channel::UpdateChannel::Beta
        );
        assert_eq!(s.library_songs_bucket, LibraryBucket::Under50);
        assert_eq!(s.theme_count_bucket, ThemeBucket::Under5);
        // The two features with no backend in any shipped build report the
        // truth, and read it from the app's own source rather than a literal.
        assert!(!s.sync_enabled);
        assert!(!s.companion_enabled);

        // A missing config file is a machine with nothing set up, not an error.
        let empty = tempfile::tempdir().expect("tempdir");
        let s = WireSettings::gather(&db.pool, empty.path()).await;
        assert_eq!(s.output_count, 0);
        assert_eq!(s.display_count_bucket, DisplayBucket::None);
        assert!(!s.fallback_in_process);
    }

    #[test]
    fn the_settings_block_has_exactly_the_ten_agreed_fields() {
        // The programme's Nøkkeldesign names ten. A field added here without the
        // Worker being deployed first would 400 every payload in the field and
        // be dropped without retry (law 3), so the count is pinned.
        let json = serde_json::to_value(WireSettings::default()).expect("serialises");
        let obj = json.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "aiConfigured",
                "companionEnabled",
                "displayCountBucket",
                "fallbackInProcess",
                "librarySongsBucket",
                "outputCount",
                "stageDisplayEnabled",
                "syncEnabled",
                "themeCountBucket",
                "updateRing",
            ]
        );
    }

    // ── The wire shape (E4: unknown fields are LOUD rejections) ──────────────

    #[test]
    fn a_quality_report_carries_no_local_bookkeeping() {
        // `QualityRow` has two fields that exist purely for this machine: `id`
        // (the SQLite primary key) and `dedupeKey` (derived from a session's
        // start time and cue count, so it says more about a service than the
        // endpoint has any business knowing). Neither is in the agreed schema,
        // and E4's validator does not ignore unknown fields — it 400s the whole
        // payload, which `classify` then drops PERMANENTLY. One stray field
        // would silently end telemetry for every install carrying it.
        let row = quality_row("018f3a2b-7c4d-7e1f-9a2b-3c4d5e6f7a8b", 5_000);
        let json = serde_json::to_value(QualityReport::from(&row)).expect("serialises");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "abnormalEnd",
                "at",
                "companionFailures",
                "connectTimeouts",
                "cueCount",
                "cueLatencyP95Ms",
                "dispatchErrors",
                "durationSec",
                "fallbackUsed",
                "outputChildRestarts",
                "reasons",
                "recovered",
                "staleChildReaped",
                "verdict",
                "watchdogHolds",
            ],
            "exactly the programme's quality fields — no more, no fewer"
        );
        // Belt and braces: the id's characters must not appear ANYWHERE in the
        // serialised report, under any key.
        let raw = serde_json::to_string(&QualityReport::from(&row)).expect("serialises");
        assert!(!raw.contains("018f3a2b"), "{raw}");
        assert!(!raw.contains("dedupe"), "{raw}");
    }

    #[test]
    fn a_quality_row_with_a_dedupe_key_still_projects_clean() {
        // The reconstruction path is the one that populates `dedupe_key`, so it
        // is the one that would leak it.
        let mut row = quality_row("q", 5_000);
        row.dedupe_key = Some("abnormal:1754600000000:37".into());
        let raw = serde_json::to_string(&QualityReport::from(&row)).expect("serialises");
        assert!(!raw.contains("abnormal:"), "{raw}");
        assert!(!raw.contains("1754600000000"), "{raw}");
    }

    #[test]
    fn a_crash_report_carries_no_record_schema() {
        // The ring's `schema` field is local versioning for the FILE format; the
        // agreed crash shape has no such key, so it is another unknown_field.
        let entry = CrashEntry {
            schema: RECORD_SCHEMA,
            kind: CrashKind::Panic,
            at: 1,
            app_version: "0.5.0".into(),
            os: TelemetryOs::Macos,
            message: "boom".into(),
            location: None,
            task: None,
            backtrace_present: true,
        };
        let json = serde_json::to_value(CrashReport::from(&entry)).expect("serialises");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "appVersion",
                "at",
                "backtracePresent",
                "kind",
                "location",
                "message",
                "os",
                "task",
            ]
        );
    }

    #[test]
    fn the_wire_projection_never_truncates_a_second_time() {
        // The crash ring writes records ALREADY at the wire caps, with the
        // ellipsis-+1 convention E4's validator checks. So this projection must
        // pass the strings through untouched: a second truncation pass here
        // would produce `max + 2` chars (an ellipsis after an ellipsis) and the
        // Worker would reject every long crash message — permanently, without
        // retry, on every machine that crashed.
        let entry = crash_ring::build_entry(
            CrashKind::Panic,
            &"a".repeat(super::super::MESSAGE_MAX_CHARS + 50),
            Some(&"b".repeat(super::super::LOCATION_MAX_CHARS + 50)),
            Some(&"c".repeat(super::super::TASK_MAX_CHARS + 50)),
            42,
            None,
        );
        // What the ring wrote: exactly cap + 1, the last char an ellipsis.
        assert_eq!(
            entry.message.chars().count(),
            super::super::MESSAGE_MAX_CHARS + 1
        );
        assert!(entry.message.ends_with('…'));

        let wire = CrashReport::from(&entry);
        assert_eq!(wire.message, entry.message, "passed through, not re-cut");
        assert_eq!(wire.location, entry.location);
        assert_eq!(wire.task, entry.task);
        assert_eq!(
            wire.message.chars().count(),
            super::super::MESSAGE_MAX_CHARS + 1,
            "still cap + 1 — one ellipsis, never two"
        );
        assert_eq!(
            wire.location.as_ref().expect("location").chars().count(),
            super::super::LOCATION_MAX_CHARS + 1
        );
        assert_eq!(
            wire.task.as_ref().expect("task").chars().count(),
            super::super::TASK_MAX_CHARS + 1
        );
        assert!(!wire.message.ends_with("……"), "{}", wire.message);
    }

    // ── The byte cap (E4: 82 % of the cap at two bytes per character) ────────

    /// A crash record whose free text is at every cap, in a script where each
    /// character is FOUR bytes. This is the shape E4's measurement warns about:
    /// the caps are counted in chars, the body cap in bytes.
    fn worst_case_crash(at: i64) -> CrashReport {
        CrashReport {
            kind: CrashKind::Panic,
            at,
            app_version: "0.5.0-beta.1".into(),
            os: TelemetryOs::Macos,
            // 🎹 is four bytes in UTF-8 and one char to `chars().count()`.
            message: "🎹".repeat(super::super::MESSAGE_MAX_CHARS),
            location: Some("🎹".repeat(super::super::LOCATION_MAX_CHARS)),
            task: Some("🎹".repeat(super::super::TASK_MAX_CHARS)),
            backtrace_present: true,
        }
    }

    /// A manual report at every cap, in four-byte script. E6 fills these; the
    /// bulk of E4's worst case is here — a 4 000-character log tail is by far
    /// the largest single thing a payload may carry.
    fn worst_case_report(at: i64) -> ProblemReport {
        ProblemReport {
            at,
            context: ReportContext::Live,
            message: "🎹".repeat(super::super::MESSAGE_MAX_CHARS),
            log_tail: "🎹".repeat(LOG_TAIL_MAX_CHARS),
        }
    }

    /// Every collection at its count cap, every string at its char cap, in a
    /// script where one character is four bytes. This is E4's worst case with
    /// the multiplier its measurement warned about.
    fn worst_case_payload() -> StagePayload {
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0-beta.1", 1_800_000_000_000);
        p.language = Some("nb".into());
        for name in crate::telemetry::counters::ALL_COUNTERS {
            p.counters.push(CounterReport {
                name,
                value: 999_999,
            });
        }
        for i in 0..MAX_CRASHES {
            p.crashes.push(worst_case_crash(i as i64));
        }
        for i in 0..MAX_QUALITY {
            p.quality
                .push(QualityReport::from(&quality_row("q", i as i64)));
        }
        for i in 0..MAX_REPORTS {
            p.reports.push(worst_case_report(i as i64));
        }
        p
    }

    #[test]
    fn a_full_payload_of_four_byte_text_overflows_the_worker_cap() {
        // The premise, asserted rather than assumed: satisfying every COUNT cap
        // does NOT keep a payload under the BYTE cap. E4 measured 53 505 bytes
        // for this shape with two-byte text — 82 % of the cap — and the caps are
        // counted in chars, so four-byte characters blow straight through it.
        let mut p = worst_case_payload();
        p.truncate_to_caps();
        let bytes = p.body_bytes();
        assert!(
            bytes > BODY_MAX_BYTES,
            "expected the count caps alone to allow an oversized body, got {bytes} bytes"
        );
        // And the half of it that E5 can actually build today (no reports) is
        // comfortably under — which is WHY the guard has to be tested through an
        // explicit target rather than waiting for E6 to make it reachable.
        p.reports.clear();
        assert!(p.body_bytes() < BODY_MAX_BYTES);
    }

    #[test]
    fn the_trim_spends_the_free_collections_before_the_lossy_one() {
        // The order is the argument: quality and reports come back in the next
        // payload, a dropped crash record does not. So the cheap ones go first
        // and the crash ring is defended.
        let mut p = worst_case_payload();
        p.truncate_to_caps();

        let fit = p.fit_to_body_cap(BODY_TARGET_BYTES);

        assert!(fit.trimmed());
        assert!(!fit.over_cap);
        assert!(
            p.body_bytes() <= BODY_TARGET_BYTES,
            "still {} bytes",
            p.body_bytes()
        );
        assert!(
            p.body_bytes() < BODY_MAX_BYTES,
            "and clear of the hard cap the Worker answers 413 to"
        );
        assert_eq!(fit.bytes, p.body_bytes(), "the report states the real size");
        assert_eq!(
            fit.crashes_dropped, 0,
            "the lossy collection survived — spending quality and reports was enough"
        );
        assert_eq!(p.crashes.len(), MAX_CRASHES);
        assert_eq!(fit.quality_dropped, MAX_QUALITY, "quality is spent first");
    }

    #[test]
    fn a_crash_only_overflow_drops_the_oldest_records() {
        // When there is nothing cheap left, crash records go — from the FRONT,
        // because a machine that crashes repeatedly crashes the same way, and
        // the newest records are the ones that describe the version in use.
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);
        for i in 0..MAX_CRASHES {
            p.crashes.push(worst_case_crash(i as i64));
        }
        let target = p.body_bytes() / 2;

        let fit = p.fit_to_body_cap(target);

        assert!(fit.crashes_dropped > 0);
        assert_eq!(fit.quality_dropped, 0);
        assert_eq!(p.crashes.len(), MAX_CRASHES - fit.crashes_dropped);
        assert_eq!(
            p.crashes.last().expect("crashes remain").at,
            MAX_CRASHES as i64 - 1,
            "the newest crash is the one that survives"
        );
        assert_eq!(
            p.crashes.first().expect("crashes remain").at,
            fit.crashes_dropped as i64,
            "dropped from the front"
        );
        assert!(p.body_bytes() <= target);
    }

    #[test]
    fn a_payload_that_already_fits_is_untouched() {
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);
        p.crashes.push(worst_case_crash(1));
        p.quality
            .push(QualityReport::from(&quality_row("q", 5_000)));
        let before = p.clone();

        let fit = p.fit_to_body_cap(BODY_TARGET_BYTES);

        assert!(!fit.trimmed());
        assert_eq!(fit.quality_dropped, 0);
        assert_eq!(fit.crashes_dropped, 0);
        assert_eq!(p, before, "a payload under the cap is not modified at all");
    }

    #[test]
    fn the_trim_never_drops_a_counter() {
        // Counters are the one collection whose entries cannot be recovered:
        // each is a DELTA, and the events behind it are gone once reported.
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);
        for name in crate::telemetry::counters::ALL_COUNTERS {
            p.counters.push(CounterReport { name, value: 3 });
        }
        for i in 0..MAX_CRASHES {
            p.crashes.push(worst_case_crash(i as i64));
        }
        p.fit_to_body_cap(BODY_TARGET_BYTES);
        assert_eq!(
            p.counters.len(),
            crate::telemetry::counters::ALL_COUNTERS.len(),
            "every counter survives whatever else has to go"
        );
    }

    #[test]
    fn the_trim_terminates_even_against_an_impossible_target() {
        // A target below the size of an empty header. There is nothing left to
        // drop, so the loop must stop and SAY so rather than spin — a hung pump
        // would be a background task quietly eating a core on a Sunday.
        let mut p = StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1_000);
        p.crashes.push(worst_case_crash(1));
        let fit = p.fit_to_body_cap(1);
        assert!(fit.over_cap);
        assert!(p.crashes.is_empty());
        assert!(fit.bytes > 1);
    }

    #[tokio::test]
    async fn a_trimmed_quality_row_is_not_marked_sent_and_returns_next_time() {
        // The lockstep property, and the reason `build_to` takes a target: if
        // the marks claimed a row the payload no longer carries, that row would
        // be stamped `sent_at` and never reported — the byte cap turning quietly
        // into data loss, which is the exact failure it was added to prevent.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        for i in 0..MAX_CRASHES {
            write_crash(
                dir.path(),
                (i + 1) as i64,
                &"🎹".repeat(super::super::MESSAGE_MAX_CHARS),
            );
        }
        for i in 0..3 {
            repo.insert_quality(&quality_row(&format!("q{i}"), 100 + i))
                .await
                .expect("quality");
        }

        // A target far below what the crash records alone need, so every
        // quality row is spent before the trim even reaches them.
        let (p, marks) = build_to(&db.pool, &ctx(dir.path(), 10_000), 0, None, 8_000)
            .await
            .expect("build");
        assert!(p.quality.is_empty(), "the quality rows were trimmed out");
        assert!(
            marks.quality_ids.is_empty(),
            "so the marks must not claim them"
        );

        repo.commit_marks(&marks).await.expect("commit");
        assert_eq!(
            repo.unsent_quality(10).await.expect("unsent").len(),
            3,
            "all three rows are still owed, and ride the next payload"
        );
    }

    #[tokio::test]
    async fn a_partial_quality_trim_marks_exactly_what_shipped() {
        // The harder half of lockstep: SOME rows go and some stay. An off-by-one
        // between the payload and the marks would either lose a row or re-send
        // one, and neither is visible without this.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        for i in 0..6 {
            repo.insert_quality(&quality_row(&format!("q{i}"), 100 + i))
                .await
                .expect("quality");
        }

        // Sized to fit only some of the six rows.
        let (p, marks) = build_to(&db.pool, &ctx(dir.path(), 10_000), 0, None, 1_200)
            .await
            .expect("build");
        assert!(!p.quality.is_empty(), "not everything was trimmed");
        assert!(p.quality.len() < 6, "and not everything survived");
        assert_eq!(
            marks.quality_ids.len(),
            p.quality.len(),
            "one mark per shipped row, no more and no fewer"
        );
        // The rows that shipped are the NEWEST ones, and the marks name exactly
        // those — matched by timestamp, which is the only field both carry.
        let shipped_ats: Vec<i64> = p.quality.iter().map(|q| q.at).collect();
        for (id, at) in marks.quality_ids.iter().zip(&shipped_ats) {
            let n: i64 = id.trim_start_matches('q').parse().expect("test id");
            assert_eq!(100 + n, *at, "mark {id} does not describe the row at {at}");
        }

        repo.commit_marks(&marks).await.expect("commit");
        assert_eq!(
            repo.unsent_quality(10).await.expect("unsent").len(),
            6 - p.quality.len(),
            "exactly the trimmed rows are still owed"
        );
    }

    #[tokio::test]
    async fn a_report_the_byte_cap_deferred_is_still_owed_afterwards() {
        // E5's condition on treating reports as a "free" trim, made true: a
        // report that does not fit keeps `sent_at` NULL and rides the next
        // payload. If the marks claimed it anyway, the byte cap would be the one
        // place an operator's hand-written words disappear — silently, and only
        // on the machines with the most to say.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        for i in 0..MAX_REPORTS {
            repo.insert_report(&StoredReport {
                id: format!("r{i}"),
                ephemeral: false,
                report: worst_case_report(100 + i as i64),
            })
            .await
            .expect("report");
        }

        // Sized so that some reports fit and some do not.
        let (p, marks) = build_to(&db.pool, &ctx(dir.path(), 10_000), 0, None, 20_000)
            .await
            .expect("build");
        assert!(!p.reports.is_empty(), "not everything was trimmed");
        assert!(p.reports.len() < MAX_REPORTS, "and not everything survived");
        assert_eq!(
            marks.report_ids.len(),
            p.reports.len(),
            "one mark per shipped report, no more and no fewer"
        );
        // The survivors are the NEWEST, and the marks name exactly those.
        for (id, r) in marks.report_ids.iter().zip(&p.reports) {
            let n: i64 = id.trim_start_matches('r').parse().expect("test id");
            assert_eq!(100 + n, r.at, "mark {id} does not describe the report");
        }

        repo.commit_marks(&marks).await.expect("commit");
        assert_eq!(
            repo.unsent_report_count().await.expect("owed"),
            (MAX_REPORTS - p.reports.len()) as i64,
            "exactly the deferred reports are still owed"
        );
    }

    #[tokio::test]
    async fn an_ephemeral_report_is_never_swept_into_an_ordinary_payload() {
        // The one-shot consent must not leak into the durable stream: a report
        // written while standing consent was off belongs to its own send, under
        // its own throwaway id, no matter what happens to the standing answer
        // later.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        TelemetryRepo::new(&db.pool)
            .insert_report(&StoredReport {
                id: "eph".into(),
                ephemeral: true,
                report: ProblemReport::new(500, ReportContext::Live, "flyktig", "", None),
            })
            .await
            .expect("report");

        let (p, marks) = build_to(
            &db.pool,
            &ctx(dir.path(), 10_000),
            0,
            None,
            BODY_TARGET_BYTES,
        )
        .await
        .expect("build");

        assert!(p.reports.is_empty(), "{:?}", p.reports);
        assert!(marks.report_ids.is_empty());
    }

    #[tokio::test]
    async fn the_crash_watermark_advances_past_dropped_records_so_the_pump_cannot_stall() {
        // The lossy half, pinned deliberately. Leaving the watermark behind a
        // dropped record would rebuild the same oversized payload every beat,
        // trim it identically, and never make progress — so the NEWEST crashes
        // would never be reported either, which is strictly worse than losing
        // the oldest few.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..MAX_CRASHES {
            write_crash(
                dir.path(),
                (i + 1) as i64,
                &"🎹".repeat(super::super::MESSAGE_MAX_CHARS),
            );
        }

        let (p, marks) = build_to(&db.pool, &ctx(dir.path(), 10_000), 0, None, 12_000)
            .await
            .expect("build");
        assert!(p.crashes.len() < MAX_CRASHES, "some had to go");
        assert_eq!(
            marks.crash_at, MAX_CRASHES as i64,
            "the watermark names the newest record READ, not the newest SENT"
        );

        // …and the next build is empty rather than an identical oversized retry.
        let (again, _) = build_to(
            &db.pool,
            &ctx(dir.path(), 11_000),
            marks.crash_at,
            None,
            12_000,
        )
        .await
        .expect("rebuild");
        assert!(again.crashes.is_empty());
    }

    #[tokio::test]
    async fn every_built_payload_fits_the_worker_body_cap() {
        // The end-to-end guarantee, at the seam that matters: whatever is on
        // this machine, `build` cannot hand the outbox something a 413 would
        // discard.
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        for i in 0..(MAX_CRASHES + 10) {
            write_crash(
                dir.path(),
                (i + 1) as i64,
                &"🎹".repeat(super::super::MESSAGE_MAX_CHARS),
            );
        }
        for i in 0..(MAX_QUALITY + 10) {
            repo.insert_quality(&quality_row(&format!("q{i}"), 100 + i as i64))
                .await
                .expect("quality");
        }
        for name in crate::telemetry::counters::ALL_COUNTERS {
            repo.add_counters(&[(name, 7)]).await.expect("counters");
        }

        let (p, _) = build(&db.pool, &ctx(dir.path(), 999_999), 0, None)
            .await
            .expect("build");
        assert!(
            p.body_bytes() <= BODY_TARGET_BYTES,
            "built {} bytes, target {BODY_TARGET_BYTES}",
            p.body_bytes()
        );
    }

    #[test]
    fn the_envelope_has_exactly_the_agreed_top_level_keys() {
        let json = serde_json::to_value(StagePayload::new(NIL_INSTALL_ID, 1, "0.5.0", 1))
            .expect("serialises");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "appVersion",
                "arch",
                "builtAt",
                "consentVersion",
                "counters",
                "crashes",
                "installId",
                "language",
                "os",
                "quality",
                "reports",
                "schema",
                "settings",
            ],
            "`reports` is present from day one — STAGE_OPTIONAL_PAYLOAD_KEYS is \
             empty, so an omitted key is a 400 the client would never retry"
        );
    }
}

/// The cross-repo probe: emit a REAL payload for E4's validator to judge.
///
/// Every test above checks this client against its own idea of the contract.
/// That is exactly the shape of bug the suite keeps finding — two layers each
/// correct on their own, disagreeing at the seam, both green. The only way to
/// know the Rust types and the TypeScript validator agree is to serialise the
/// real types and let the real validator read them.
///
/// ## How to run it
///
/// ```sh
/// cargo test --lib emit_maximal_payload -- --ignored --nocapture \
///   | sed -n '/---PAYLOAD---/,$p' | sed -n '2p' > /tmp/payload.json
/// ```
///
/// then, in a `sunday-telemetry` checkout, feed `/tmp/payload.json` to
/// `validateStagePayload` from `src/schema/sundaystage.ts` (inline the JSON —
/// the Workers test pool has no host filesystem).
///
/// ## What it found on 2026-08-09
///
/// Run against the deployed schema (`sunday-telemetry` main `994b337`), this
/// caught THREE real breaks before a byte was ever sent:
///
/// * the band vocabularies this file first invented (`"0"`, `"1-9"`, `"10-49"`)
///   → `not_in_enum` on all three settings fields. E4's deployed lists are
///   `none|one|two|three|four_plus`, `under_50|50_200|…`, `under_5|5_20|…`;
/// * a `QualityRow` projected whole → `unknown_field` on `id` and `dedupeKey`;
/// * the nil install id → `nil_install_id`, which is why [`super::client::drain`]
///   mints rather than falling back to a placeholder.
///
/// Every one of those is a 400, and a 400 is dropped PERMANENTLY without retry.
/// The whole feature would have been silently dead in the field.
#[cfg(test)]
mod crossrepo_probe {
    use super::*;
    use crate::db::Database;

    /// Dump a maximal payload — every collection populated, every crash kind,
    /// a report at its caps — through the REAL builder.
    #[tokio::test]
    #[ignore = "emits a fixture for the cross-repo probe; see the module docs"]
    async fn emit_maximal_payload() {
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TelemetryRepo::new(&db.pool);
        // Consent FIRST: granting stamps everything already on disk as reported,
        // so data inserted afterwards is what a real payload would carry.
        crate::telemetry::client::consent_set(&db.pool, true)
            .await
            .expect("grant");
        for name in crate::telemetry::counters::ALL_COUNTERS {
            repo.add_counters(&[(name, 7)]).await.expect("counters");
        }
        let mut row = crate::telemetry::quality::QualityRow {
            id: crate::db::new_id(),
            dedupe_key: Some("abnormal:1:2".into()),
            at: 1_800_000_000_000,
            duration_sec: 3_600,
            cue_count: 42,
            output_child_restarts: 1,
            connect_timeouts: 0,
            watchdog_holds: 1,
            dispatch_errors: 0,
            companion_failures: 0,
            fallback_used: true,
            stale_child_reaped: true,
            abnormal_end: true,
            recovered: true,
            cue_latency_p95_ms: Some(250),
            verdict: crate::telemetry::quality::QualityVerdict::Fail,
            reasons: vec![
                crate::telemetry::quality::QualityReason::AbnormalEnd,
                crate::telemetry::quality::QualityReason::OutputRestarted,
            ],
        };
        repo.insert_quality(&row).await.expect("q1");
        row.id = crate::db::new_id();
        row.dedupe_key = None;
        row.at = 1_800_000_001_000;
        row.cue_latency_p95_ms = None;
        row.verdict = crate::telemetry::quality::QualityVerdict::Pass;
        row.reasons = vec![crate::telemetry::quality::QualityReason::Clean];
        repo.insert_quality(&row).await.expect("q2");

        let ring = crash_ring::crash_dir(dir.path());
        std::fs::create_dir_all(&ring).expect("ring");
        for (i, kind) in [
            CrashKind::Panic,
            CrashKind::TaskPanic,
            CrashKind::WebviewError,
            CrashKind::UnhandledRejection,
            CrashKind::Other,
        ]
        .into_iter()
        .enumerate()
        {
            let e = crash_ring::build_entry(
                kind,
                &"æ".repeat(super::super::MESSAGE_MAX_CHARS + 20),
                Some("src/live.rs:12:3"),
                Some("output-supervisor"),
                1_800_000_000_000 + i as i64,
                None,
            );
            std::fs::write(
                ring.join(format!(
                    "crash-{:015}-0000.json",
                    1_800_000_000_000i64 + i as i64
                )),
                serde_json::to_vec(&e).expect("ser"),
            )
            .expect("write");
        }

        crate::telemetry::client::set_language(&db.pool, "nb-NO")
            .await
            .expect("lang");
        crate::telemetry::client::set_crash_watermark(&db.pool, 0)
            .await
            .expect("watermark");
        let id = crate::telemetry::client::install_id_if_any(&db.pool)
            .await
            .expect("read")
            .expect("id");

        let (mut p, _) = build(
            &db.pool,
            &GatherContext {
                app_data_dir: dir.path(),
                app_version: "0.6.0-beta.1",
                home: None,
                now_ms: 1_800_000_100_000,
                consent_version: 1,
            },
            0,
            Some(&id),
        )
        .await
        .expect("build");
        // E6's collection, exercised here so the probe covers the whole shape.
        p.reports.push(ProblemReport::new(
            1_800_000_050_000,
            ReportContext::Live,
            &"ø".repeat(super::super::MESSAGE_MAX_CHARS + 5),
            &"log line\n".repeat(600),
            None,
        ));
        println!("---PAYLOAD---");
        println!("{}", serde_json::to_string(&p).expect("serialise"));
    }
}
