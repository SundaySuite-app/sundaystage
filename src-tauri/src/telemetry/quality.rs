//! E3 — the session-quality collector. **This is the module law 1 is about.**
//!
//! > *The live path is sacred.* Telemetry on the live path is atomic increments
//! > and bounded `try_send` only — never disk, never a lock, never async, never
//! > a `Result` the caller has to handle. The collector drains only when
//! > `AppState.live` is `None`; a try-lock MISS counts as live.
//!
//! Every signal in a quality row is already computed today and thrown away: the
//! supervisor counts child restarts, the connect timeout fires, the companion
//! publish fails, the fallback to in-process windows happens. E3 keeps them.
//!
//! ## The shape of the guarantee
//!
//! Three separate mechanisms, because one would be a promise and three are a
//! design:
//!
//!   1. **The API cannot block, by type.** Everything the live path may call is
//!      on [`LiveSafe`], whose every method returns `()`. There is no `Result`
//!      to handle, so there is nothing a caller could be tempted to `?` — and a
//!      method that needed to report failure could not be added to the trait
//!      without changing its shape. [`the_live_safe_surface_cannot_block`]
//!      reads this file back and fails on `.await`, `.lock()`, `-> Result` or a
//!      filesystem call inside the implementation.
//!   2. **The events go into fixed atomics and a bounded channel.** A full
//!      channel drops the oldest news, it never waits.
//!   3. **The drain asks the live mutex first**, with `try_lock`, and treats a
//!      MISS as live. A miss means somebody is inside a live command right now;
//!      that is the exact moment not to touch a disk.
//!
//! ## Cue latency
//!
//! The `< 50 ms p95` cue-advance promise is measured here, seq-correlated:
//! `OutputSupervisor::render` already returns the `seq` it assigned and the
//! child already ACKs that exact `seq`, so the round trip needs no new protocol
//! — one atomic store on dispatch, one compare-exchange on ACK, into a FIXED
//! six-bucket histogram. No allocation, no timestamp map, no clock syscall
//! beyond a monotonic `Instant::elapsed`.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::time::Instant;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::live_session::LiveSession;
use crate::telemetry::counters::{CounterBuffer, CounterName};

/// How many finished rows are held in memory before the oldest news is
/// dropped. Matches the payload cap E4 pins server-side, so a machine that
/// somehow ran twenty services without a quiet moment loses the oldest rather
/// than growing without bound.
pub const BUFFER_MAX: usize = 20;

/// Upper bounds (inclusive, ms) of the cue-latency histogram's first five
/// buckets; everything else lands in a sixth, open-ended bucket.
pub const LATENCY_BOUNDS_MS: [u64; 5] = [10, 25, 50, 100, 250];

/// Number of histogram buckets: the five bounded ones plus the overflow.
pub const LATENCY_BUCKETS: usize = LATENCY_BOUNDS_MS.len() + 1;

/// How many already-written row ids the drain remembers, so a row that somehow
/// reaches it twice is written once. Comfortably more than [`BUFFER_MAX`], the
/// most rows a single pass can hold, and bounded so a long-running machine
/// cannot grow it.
const WRITTEN_ID_MEMORY: usize = BUFFER_MAX * 4;

/// Cue-advance budget from CLAUDE.md. A session whose p95 exceeds this earns
/// the `slow-cues` reason — the first honest measurement of core promise #1.
pub const CUE_BUDGET_MS: u32 = 50;

/// In-flight dispatch slots for seq correlation. `seq` increments by one per
/// render and an ACK comes back in milliseconds, so 64 is many services' worth
/// of head-room; the slot also stores its `seq`, so a wrap can only ever DROP a
/// measurement, never mis-attribute one.
const SEQ_SLOTS: usize = 64;

// ─────────────────────────────────────────────────────────────────────────────
//   The row
// ─────────────────────────────────────────────────────────────────────────────

/// The Pass/Warn/Fail verdict for one live session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/QualityVerdict.ts")]
#[serde(rename_all = "lowercase")]
pub enum QualityVerdict {
    /// Nothing went wrong.
    Pass,
    /// Something the congregation may have noticed, but the service ran.
    Warn,
    /// The session did not end cleanly.
    Fail,
}

impl QualityVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

    pub fn from_wire(raw: &str) -> Option<Self> {
        match raw {
            "pass" => Some(Self::Pass),
            "warn" => Some(Self::Warn),
            "fail" => Some(Self::Fail),
            _ => None,
        }
    }
}

/// WHY a session reached its verdict — a CLOSED set of codes, never prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/QualityReason.ts")]
#[serde(rename_all = "kebab-case")]
pub enum QualityReason {
    /// An isolated output child died and was restarted.
    OutputRestarted,
    /// An output went long enough without a heartbeat that its watchdog is
    /// holding the last frame — the congregation saw a frozen slide.
    HoldLastFrame,
    /// The session did not end through `live_end`.
    AbnormalEnd,
    /// A cue dispatch or an output child reported an error.
    DispatchErrors,
    /// The cue-advance p95 exceeded [`CUE_BUDGET_MS`].
    SlowCues,
    /// A companion broadcast failed.
    CompanionFailures,
    /// The outputs ran as in-process windows, without crash isolation.
    FallbackUsed,
    /// Nothing fired. Present so `reasons` is never empty.
    Clean,
}

impl QualityReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OutputRestarted => "output-restarted",
            Self::HoldLastFrame => "hold-last-frame",
            Self::AbnormalEnd => "abnormal-end",
            Self::DispatchErrors => "dispatch-errors",
            Self::SlowCues => "slow-cues",
            Self::CompanionFailures => "companion-failures",
            Self::FallbackUsed => "fallback-used",
            Self::Clean => "clean",
        }
    }

    pub fn from_wire(raw: &str) -> Option<Self> {
        [
            Self::OutputRestarted,
            Self::HoldLastFrame,
            Self::AbnormalEnd,
            Self::DispatchErrors,
            Self::SlowCues,
            Self::CompanionFailures,
            Self::FallbackUsed,
            Self::Clean,
        ]
        .into_iter()
        .find(|r| r.as_str() == raw)
    }
}

/// How a live session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOutcome {
    /// Through `live_end`.
    Clean,
    /// Through `live_recover` — this session was resumed from a WAL.
    Recovered,
    /// Reconstructed at startup: the previous process never reached `live_end`.
    Abnormal,
}

/// One finished live session's health.
///
/// Numbers, booleans and closed enums only: no service name, no song title, no
/// display name, no path. What a support conversation needs from a Sunday is
/// whether the projector kept up, and these are the numbers that answer it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/QualityRow.ts")]
#[serde(rename_all = "camelCase")]
pub struct QualityRow {
    /// UUIDv7, per the repository convention.
    pub id: String,
    /// A stable key for rows that can be produced twice (the startup
    /// reconstruction sees the same leftover WAL on every launch until it is
    /// resumed or cleared). `None` for ordinary rows. LOCAL ONLY — E5's payload
    /// builder does not project it.
    pub dedupe_key: Option<String>,
    /// Unix ms the session ended.
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
    /// The outputs ran as in-process windows rather than isolated processes.
    pub fallback_used: bool,
    /// A child left over from a crashed previous process was killed.
    pub stale_child_reaped: bool,
    pub abnormal_end: bool,
    pub recovered: bool,
    /// Interpolated 95th percentile of dispatch→ACK, or `None` when no cue was
    /// ACKed (no isolated outputs were open, or the service had no cues).
    #[ts(type = "number | null")]
    pub cue_latency_p95_ms: Option<u32>,
    pub verdict: QualityVerdict,
    /// Never empty: a clean session carries `[clean]`.
    pub reasons: Vec<QualityReason>,
}

/// Derive the verdict and its reasons from the numbers. Pure, so every rule is
/// provable without running a service.
fn judge(row: &QualityRow) -> (QualityVerdict, Vec<QualityReason>) {
    let mut reasons = Vec::new();
    if row.abnormal_end {
        reasons.push(QualityReason::AbnormalEnd);
    }
    if row.output_child_restarts > 0 {
        reasons.push(QualityReason::OutputRestarted);
    }
    if row.watchdog_holds > 0 {
        reasons.push(QualityReason::HoldLastFrame);
    }
    if row.dispatch_errors > 0 {
        reasons.push(QualityReason::DispatchErrors);
    }
    if row.cue_latency_p95_ms.is_some_and(|p| p > CUE_BUDGET_MS) {
        reasons.push(QualityReason::SlowCues);
    }
    if row.companion_failures > 0 {
        reasons.push(QualityReason::CompanionFailures);
    }
    if row.fallback_used {
        reasons.push(QualityReason::FallbackUsed);
    }
    // An abnormal end is the only FAIL: everything else means the service ran
    // and something was worth noticing. A restarted output child is explicitly
    // a warn, not a fail — hold-last-frame covered the gap, which is the whole
    // reason the child is a separate process.
    let verdict = if row.abnormal_end {
        QualityVerdict::Fail
    } else if reasons.is_empty() {
        QualityVerdict::Pass
    } else {
        QualityVerdict::Warn
    };
    if reasons.is_empty() {
        reasons.push(QualityReason::Clean);
    }
    (verdict, reasons)
}

// ─────────────────────────────────────────────────────────────────────────────
//   Cue latency
// ─────────────────────────────────────────────────────────────────────────────

/// A fixed histogram of dispatch→ACK deltas.
///
/// Increments only. Nothing here allocates, takes a lock, or calls the wall
/// clock — `Instant::elapsed` on a process-wide base is a monotonic counter
/// read.
#[derive(Debug)]
struct CueLatency {
    base: Instant,
    /// `seq` awaiting an ACK in this slot; `0` = free (the supervisor's `seq`
    /// starts at 1, so zero is a safe sentinel).
    slot_seq: [AtomicU64; SEQ_SLOTS],
    /// Microseconds since `base` when that `seq` was dispatched.
    slot_at: [AtomicU64; SEQ_SLOTS],
    buckets: [AtomicU64; LATENCY_BUCKETS],
}

impl CueLatency {
    fn new() -> Self {
        Self {
            base: Instant::now(),
            slot_seq: std::array::from_fn(|_| AtomicU64::new(0)),
            slot_at: std::array::from_fn(|_| AtomicU64::new(0)),
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    fn now_us(&self) -> u64 {
        self.base.elapsed().as_micros() as u64
    }

    /// A frame went out with this `seq`. Two relaxed stores.
    fn dispatched(&self, seq: u64) {
        if seq == 0 {
            return;
        }
        let i = (seq as usize) % SEQ_SLOTS;
        self.slot_at[i].store(self.now_us(), Ordering::Relaxed);
        // The seq is published LAST (Release), so an ACK that sees it also sees
        // the timestamp beside it.
        self.slot_seq[i].store(seq, Ordering::Release);
    }

    /// A child ACKed this `seq`. One compare-exchange claims the slot, so with
    /// several outputs open only the FIRST child's ACK is measured — the second
    /// screen is not a second data point about the same keypress.
    fn acked(&self, seq: u64) {
        if seq == 0 {
            return;
        }
        let i = (seq as usize) % SEQ_SLOTS;
        if self.slot_seq[i]
            .compare_exchange(seq, 0, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            // Either already claimed, or the slot was reused by a later seq
            // (more than SEQ_SLOTS renders since dispatch). Dropping the
            // measurement is correct; attributing it would not be.
            return;
        }
        let started = self.slot_at[i].load(Ordering::Acquire);
        let ms = self.now_us().saturating_sub(started) / 1_000;
        self.buckets[bucket_of(ms)].fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> [u64; LATENCY_BUCKETS] {
        std::array::from_fn(|i| self.buckets[i].load(Ordering::Relaxed))
    }

    fn reset(&self) {
        for b in &self.buckets {
            b.store(0, Ordering::Relaxed);
        }
        for s in &self.slot_seq {
            s.store(0, Ordering::Relaxed);
        }
    }
}

/// Which bucket a millisecond delta lands in.
fn bucket_of(ms: u64) -> usize {
    LATENCY_BOUNDS_MS
        .iter()
        .position(|b| ms <= *b)
        .unwrap_or(LATENCY_BOUNDS_MS.len())
}

/// The interpolated 95th percentile of a bucket histogram, in ms.
///
/// Linear inside the bucket the percentile falls in, which is the standard
/// reading of a fixed histogram. `None` when nothing was measured.
///
/// **Saturating, honestly:** a p95 that lands in the open-ended `>250 ms`
/// bucket is reported as 250. The histogram simply does not know how much worse
/// it was — and by then the `slow-cues` reason (which fires above 50 ms) has
/// already carried the finding, so buying a wider range with an unbounded
/// bucket would add bytes and no information.
pub fn p95_from_buckets(buckets: &[u64; LATENCY_BUCKETS]) -> Option<u32> {
    let total: u64 = buckets.iter().sum();
    if total == 0 {
        return None;
    }
    // The rank of the 95th percentile, 1-based, rounded up.
    let target = total.saturating_mul(95).div_ceil(100).max(1);
    let mut cumulative = 0u64;
    for (i, count) in buckets.iter().enumerate() {
        let before = cumulative;
        cumulative += count;
        if cumulative < target || *count == 0 {
            continue;
        }
        let lower = if i == 0 { 0 } else { LATENCY_BOUNDS_MS[i - 1] };
        let Some(upper) = LATENCY_BOUNDS_MS.get(i).copied() else {
            // The overflow bucket: report its lower edge and say no more.
            return Some(lower as u32);
        };
        let needed = target - before; // 1..=count
        let frac = needed as f64 / *count as f64;
        let value = lower as f64 + (upper - lower) as f64 * frac;
        return Some(value.round() as u32);
    }
    // Unreachable while `target <= total`, but a percentile function must never
    // panic on the live path's data.
    Some(LATENCY_BOUNDS_MS[LATENCY_BOUNDS_MS.len() - 1] as u32)
}

// ─────────────────────────────────────────────────────────────────────────────
//   The collector
// ─────────────────────────────────────────────────────────────────────────────

/// Everything one in-flight session accumulates. Atomics only — no field here
/// needs a lock to update, and none is ever read on the live path.
#[derive(Debug, Default)]
struct SessionCounters {
    started_at: AtomicI64,
    cues: AtomicU64,
    output_restarts: AtomicU64,
    connect_timeouts: AtomicU64,
    watchdog_holds: AtomicU64,
    dispatch_errors: AtomicU64,
    companion_failures: AtomicU64,
    stale_child_reaped: AtomicBool,
    /// This session was resumed from a crash-recovery WAL. Set by
    /// `live_recover`, cleared by the next `begin_session`, and read at
    /// `finish_session` — so `live_end` does not need to know how the session
    /// it is ending came into being.
    recovered: AtomicBool,
    /// Sticky across sessions: it describes how the outputs are RUNNING, not
    /// something that happened during one service. Cleared when the outputs
    /// next open with process isolation.
    fallback_used: AtomicBool,
}

/// The public surface the LIVE PATH may call.
///
/// Every method returns `()`. That is the point, and it is enforced three ways:
/// by this declaration, by
/// [`the_live_safe_surface_cannot_block`](tests::the_live_safe_surface_cannot_block)
/// reading the implementation back, and by the drain being a different function
/// entirely that lives behind the live gate.
///
/// A caller cannot handle an error, retry, wait, or learn that anything was
/// dropped — because on the live path there is nothing useful it could do with
/// any of that, and every branch it might take is a branch between a keypress
/// and a projector.
pub trait LiveSafe {
    /// A service went live at `at_ms`.
    fn begin_session(&self, at_ms: i64);
    /// The session that just began was resumed from a crash-recovery WAL.
    fn note_session_recovered(&self);
    /// One operator cue action was dispatched.
    fn note_cue_dispatched(&self);
    /// A frame went to the isolated outputs with this `seq`.
    fn note_render(&self, seq: u64);
    /// An output child ACKed this `seq`.
    fn note_ack(&self, seq: u64);
    /// An isolated output child died and is being restarted.
    fn note_output_restart(&self);
    /// A spawned child never connected inside the connect timeout.
    fn note_connect_timeout(&self);
    /// An output has been silent long enough that its watchdog holds the frame.
    fn note_watchdog_hold(&self);
    /// A cue dispatch failed, or an output child reported a render error.
    fn note_dispatch_error(&self);
    /// A companion broadcast failed.
    fn note_companion_failure(&self);
    /// The outputs opened as in-process windows (no crash isolation).
    fn note_fallback_in_process(&self);
    /// The outputs opened as isolated processes — clears the sticky fallback.
    fn note_isolated_outputs(&self);
    /// A child left over from a crashed previous process was killed.
    fn note_stale_child_reaped(&self);
    /// Bump one counter from the closed vocabulary.
    fn note_counter(&self, name: CounterName);
    /// The session ended. Builds one row and buffers it.
    fn finish_session(&self, at_ms: i64, outcome: SessionOutcome);
}

/// The in-memory collector. One per process, held in `AppState`.
pub struct QualityCollector {
    session: SessionCounters,
    latency: CueLatency,
    counters: CounterBuffer,
    /// Finished rows waiting for a quiet moment. Bounded; `try_send` only.
    tx: SyncSender<QualityRow>,
    /// Read ONLY by the drain, i.e. only behind the live gate.
    rx: Mutex<Receiver<QualityRow>>,
    /// Rows dropped because the buffer was full — itself a finding.
    dropped: AtomicU64,
    /// Ids of rows already persisted in this process, oldest first.
    ///
    /// The drain's idempotency key, and it is an ID rather than a timestamp on
    /// purpose. It used to be a forward-only `at` watermark, which quietly LOST
    /// work: a row put back after a transient write error is re-buffered at the
    /// BACK of the queue, so a newer row written in the same pass pushed the
    /// watermark past it and every later pass skipped it — without writing it
    /// and without putting it back. An id matches the one row it names, so a
    /// retry is never mistaken for a duplicate. The database's `INSERT OR
    /// IGNORE` on the `id` primary key (and the partial unique index on
    /// `dedupe_key`) is the second, authoritative line of the same defence.
    ///
    /// Only ever touched by the drain, i.e. behind the live gate.
    written_ids: Mutex<VecDeque<String>>,
}

impl Default for QualityCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl QualityCollector {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel::<QualityRow>(BUFFER_MAX);
        Self {
            session: SessionCounters::default(),
            latency: CueLatency::new(),
            counters: CounterBuffer::new(),
            tx,
            rx: Mutex::new(rx),
            dropped: AtomicU64::new(0),
            written_ids: Mutex::new(VecDeque::new()),
        }
    }

    /// The counter buffer, for the read command.
    pub fn counters(&self) -> &CounterBuffer {
        &self.counters
    }

    /// Rows dropped because the buffer filled before a quiet moment arrived.
    pub fn dropped_rows(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Buffer a row built elsewhere — the startup reconstruction's route in.
    /// Not on [`LiveSafe`] because nothing on the live path may call it.
    pub fn buffer_row(&self, row: QualityRow) {
        if self.tx.try_send(row).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Empty the buffer, oldest first.
    ///
    /// The receiver lock lives here and NOWHERE on the live path — nothing in
    /// [`LiveSafe`] can reach it, so a slow reader can never make a cue wait.
    pub fn take_buffered_rows(&self) -> Vec<QualityRow> {
        let rx = self.rx.lock();
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    /// Has this exact row already been persisted in this process?
    fn already_written(&self, id: &str) -> bool {
        self.written_ids.lock().iter().any(|seen| seen == id)
    }

    /// Remember a persisted row's id, dropping the oldest memory once the ring
    /// is full.
    fn remember_written(&self, id: &str) {
        let mut ids = self.written_ids.lock();
        while ids.len() >= WRITTEN_ID_MEMORY {
            ids.pop_front();
        }
        ids.push_back(id.to_string());
    }
}

impl LiveSafe for QualityCollector {
    fn begin_session(&self, at_ms: i64) {
        // EVERY per-session accumulator, not just the clock and the two flags.
        // A session that begins while another is still open — `live_start`
        // replaces a running session, and the operator who rehearses at 09:40
        // and goes live at 11:00 does exactly that — would otherwise hand the
        // rehearsal's cues, restarts and failures to the service's row, and the
        // rehearsal would never get a row of its own. (`live_start` now also
        // FINISHES the session it replaces, so the rehearsal keeps its row;
        // this reset is the other half, and neither one alone is enough.)
        self.session.started_at.store(at_ms, Ordering::Relaxed);
        self.session.cues.store(0, Ordering::Relaxed);
        self.session.output_restarts.store(0, Ordering::Relaxed);
        self.session.connect_timeouts.store(0, Ordering::Relaxed);
        self.session.watchdog_holds.store(0, Ordering::Relaxed);
        self.session.dispatch_errors.store(0, Ordering::Relaxed);
        self.session.companion_failures.store(0, Ordering::Relaxed);
        self.session.recovered.store(false, Ordering::Relaxed);
        // `fallback_used` and `stale_child_reaped` are deliberately NOT reset:
        // both describe how the OUTPUTS came up (see their fields), not
        // something that happened during one service, and the outputs are
        // routinely opened before the first `live_start` of the morning.
        self.latency.reset();
        self.counters.note(CounterName::LiveSessionStarted);
    }

    fn note_session_recovered(&self) {
        self.session.recovered.store(true, Ordering::Relaxed);
    }

    fn note_cue_dispatched(&self) {
        self.session.cues.fetch_add(1, Ordering::Relaxed);
        self.counters.note(CounterName::LiveCueDispatched);
    }

    fn note_render(&self, seq: u64) {
        self.latency.dispatched(seq);
    }

    fn note_ack(&self, seq: u64) {
        self.latency.acked(seq);
    }

    fn note_output_restart(&self) {
        self.session.output_restarts.fetch_add(1, Ordering::Relaxed);
    }

    fn note_connect_timeout(&self) {
        self.session
            .connect_timeouts
            .fetch_add(1, Ordering::Relaxed);
    }

    fn note_watchdog_hold(&self) {
        self.session.watchdog_holds.fetch_add(1, Ordering::Relaxed);
    }

    fn note_dispatch_error(&self) {
        self.session.dispatch_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn note_companion_failure(&self) {
        self.session
            .companion_failures
            .fetch_add(1, Ordering::Relaxed);
    }

    fn note_fallback_in_process(&self) {
        self.session.fallback_used.store(true, Ordering::Relaxed);
        self.counters.note(CounterName::OutputFallbackInProcess);
    }

    fn note_isolated_outputs(&self) {
        self.session.fallback_used.store(false, Ordering::Relaxed);
    }

    fn note_stale_child_reaped(&self) {
        self.session
            .stale_child_reaped
            .store(true, Ordering::Relaxed);
    }

    fn note_counter(&self, name: CounterName) {
        self.counters.note(name);
    }

    fn finish_session(&self, at_ms: i64, outcome: SessionOutcome) {
        let started_at = self.session.started_at.swap(0, Ordering::Relaxed);
        let abnormal_end = matches!(outcome, SessionOutcome::Abnormal);
        // `live_end` always says `Clean`; whether this session was a RESUME was
        // recorded when it began, so the ending command needs no memory.
        let recovered = self.session.recovered.swap(false, Ordering::Relaxed)
            || matches!(outcome, SessionOutcome::Recovered);
        let mut row = QualityRow {
            id: uuid::Uuid::now_v7().to_string(),
            dedupe_key: None,
            at: at_ms,
            duration_sec: if started_at > 0 {
                (at_ms - started_at).max(0) / 1_000
            } else {
                0
            },
            cue_count: self.session.cues.swap(0, Ordering::Relaxed) as i64,
            output_child_restarts: self.session.output_restarts.swap(0, Ordering::Relaxed) as i64,
            connect_timeouts: self.session.connect_timeouts.swap(0, Ordering::Relaxed) as i64,
            watchdog_holds: self.session.watchdog_holds.swap(0, Ordering::Relaxed) as i64,
            dispatch_errors: self.session.dispatch_errors.swap(0, Ordering::Relaxed) as i64,
            companion_failures: self.session.companion_failures.swap(0, Ordering::Relaxed) as i64,
            fallback_used: self.session.fallback_used.load(Ordering::Relaxed),
            stale_child_reaped: self
                .session
                .stale_child_reaped
                .swap(false, Ordering::Relaxed),
            abnormal_end,
            recovered,
            cue_latency_p95_ms: p95_from_buckets(&self.latency.snapshot()),
            verdict: QualityVerdict::Pass,
            reasons: Vec::new(),
        };
        let (verdict, reasons) = judge(&row);
        row.verdict = verdict;
        row.reasons = reasons;
        self.latency.reset();
        self.counters.note(if recovered {
            CounterName::LiveSessionRecovered
        } else {
            CounterName::LiveSessionEnded
        });
        // The only channel operation on this path: bounded, non-blocking, and
        // the row is dropped (and counted) rather than waited on.
        if self.tx.try_send(row).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   The drain — everything below this line is BEHIND the live gate
// ─────────────────────────────────────────────────────────────────────────────

/// Where a drained row goes. A trait so the live-gate tests can inject a sink
/// that panics if it is ever touched — the gate is proved by the sink NOT
/// being reached, which is only observable if the sink can object.
#[async_trait::async_trait]
pub trait QualityStore: Send + Sync {
    async fn write_quality(&self, row: &QualityRow) -> Result<(), String>;
    async fn add_counters(&self, deltas: &[(CounterName, u64)]) -> Result<(), String>;
}

/// What a drain pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrainReport {
    /// Rows written to the store.
    pub rows_written: usize,
    /// Counter names folded into the store.
    pub counters_written: usize,
    /// The gate said "a service is running" and nothing was touched.
    pub skipped_live: bool,
    /// Rows the store refused; they are put back for the next pass.
    pub write_failures: usize,
}

/// Whether the live gate is open — i.e. no service is running RIGHT NOW.
///
/// A `try_lock` MISS is treated as live, and that is the conservative reading
/// on purpose: a miss means some other thread is inside a live command at this
/// instant. `AppState.live` is a `parking_lot::Mutex`, which cannot be
/// poisoned, so a miss is never a permanent state — the next pass simply tries
/// again (see the poisoning test in `lib.rs`).
pub fn gate_is_open(live: &Mutex<Option<LiveSession>>) -> bool {
    match live.try_lock() {
        Some(guard) => guard.is_none(),
        None => false,
    }
}

impl QualityCollector {
    /// Persist everything buffered — but ONLY when no live session is running.
    ///
    /// Returns without touching `store` when the gate is closed, so a `store`
    /// that panics on contact is a valid way to prove the gate holds.
    ///
    /// Idempotent by row ID: a row this process has already persisted is
    /// skipped, so two passes over the same row write once — while a row the
    /// store REFUSED keeps its place in the queue and is retried, however many
    /// newer rows went in ahead of it.
    pub async fn drain_if_quiet<S: QualityStore + ?Sized>(
        &self,
        live: &Mutex<Option<LiveSession>>,
        store: &S,
    ) -> DrainReport {
        let mut report = DrainReport::default();
        if !gate_is_open(live) {
            report.skipped_live = true;
            return report;
        }
        // Take everything out of the channel FIRST, synchronously, so the
        // receiver lock is never held across an await.
        let pending = self.take_buffered_rows();
        let deltas = self.counters.take_deltas();

        for row in pending {
            // Re-check between rows: a service can go live mid-drain, and the
            // remaining rows belong in the buffer, not on the disk.
            if !gate_is_open(live) {
                report.skipped_live = true;
                self.buffer_row(row);
                continue;
            }
            if self.already_written(&row.id) {
                continue;
            }
            match store.write_quality(&row).await {
                Ok(()) => {
                    report.rows_written += 1;
                    self.remember_written(&row.id);
                }
                // The store hands back an error CODE, never a message: a
                // sqlx error can quote the failing statement, and this line
                // reaches the uploadable log tail (law 2).
                Err(code) => {
                    report.write_failures += 1;
                    tracing::warn!(%code, "quality row could not be persisted");
                    self.buffer_row(row);
                }
            }
        }

        if !deltas.is_empty() {
            // The same re-check the row loop does, for the same reason: a
            // service can go live between the last row and this write, and law
            // 1 is about touching the disk while one is running — not about
            // which KIND of write it is. On a miss the deltas go straight back,
            // exactly as a failed write returns them.
            if !gate_is_open(live) {
                report.skipped_live = true;
                self.counters.restore(&deltas);
                return report;
            }
            match store.add_counters(&deltas).await {
                Ok(()) => report.counters_written = deltas.len(),
                Err(code) => {
                    tracing::warn!(%code, "counters could not be persisted");
                    // Put them back: a transient disk error must not lose counts.
                    self.counters.restore(&deltas);
                }
            }
        }
        report
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   Startup reconstruction
// ─────────────────────────────────────────────────────────────────────────────

/// Rebuild a quality row for a session the PREVIOUS process never finished.
///
/// Two independent traces say a service was interrupted:
///
///   * `live_session.log` still exists — `live_end` deletes it, so its presence
///     means the process died mid-service;
///   * an output pidfile is still on disk — a child that outlived its parent,
///     which by design is holding the last frame it was given. Scanned under
///     `data_dir` (see [`crate::output::process::pidfile_dir`]), which is why
///     this signal is real on Windows too since 0.6.
///
/// The WAL is only READ here: `live_recover` still needs it to offer a resume.
/// That means the same leftover is seen again on the next launch if the
/// operator never resumes, which is what [`QualityRow::dedupe_key`] is for.
pub fn reconstruct_previous_session(data_dir: &Path, at_ms: i64) -> Option<QualityRow> {
    let store = crate::services::session_store::SessionStore::in_dir(data_dir);
    let stale_child_reaped = crate::output::process::stale_pidfiles_present(data_dir);
    if !store.exists() {
        // A stale child with no WAL means outputs were open without a service
        // live — nothing to say about a session that never started.
        return None;
    }
    let session = store.recover()?;
    // When the session ENDED, which is not when this launch happened. The WAL's
    // last append is the last moment the previous process is known to have been
    // alive; `at_ms` is merely when we noticed. A crash on Sunday and a relaunch
    // the following Saturday would otherwise be reported as a six-day service.
    // With no readable mtime we say ZERO — the same "unknown" the ordinary row
    // uses when it never saw a start — rather than a number we made up.
    let duration_sec = store
        .last_activity_at()
        .filter(|end| *end >= session.started_at && *end <= at_ms)
        .map_or(0, |end| (end - session.started_at) / 1_000);
    let mut row = QualityRow {
        id: uuid::Uuid::now_v7().to_string(),
        dedupe_key: Some(format!(
            "abnormal:{}:{}",
            session.started_at,
            session.log.len()
        )),
        at: at_ms,
        duration_sec,
        cue_count: session.log.len() as i64,
        output_child_restarts: 0,
        connect_timeouts: 0,
        // A child still holding a frame at launch IS the hold-last-frame case
        // that the isolation design exists for; count it as one hold.
        watchdog_holds: i64::from(stale_child_reaped),
        dispatch_errors: 0,
        companion_failures: 0,
        fallback_used: false,
        stale_child_reaped,
        abnormal_end: true,
        recovered: false,
        cue_latency_p95_ms: None,
        verdict: QualityVerdict::Fail,
        reasons: Vec::new(),
    };
    let (verdict, reasons) = judge(&row);
    row.verdict = verdict;
    row.reasons = reasons;
    Some(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cue_list::CueList;
    use crate::services::live_session::{LiveAction, LiveSession};

    fn empty_session() -> LiveSession {
        LiveSession::new(
            "svc",
            CueList {
                service_id: "svc".into(),
                compiled_at: 0,
                cues: vec![],
            },
            1_000,
        )
    }

    // ── (1) the API cannot block, by construction ────────────────────────────

    /// Read this very file back and prove the live-safe surface is what it
    /// claims to be.
    ///
    /// A doc comment saying "never blocks" is a hope. This is the check: every
    /// method on [`LiveSafe`] returns `()` (no `->` in any signature), and the
    /// implementation block contains no `.await`, no `.lock(`, no `Result`, no
    /// `std::fs`, no `send(` that is not a `try_send`, and no sleep.
    #[test]
    fn the_live_safe_surface_cannot_block() {
        let source = include_str!("quality.rs");

        // (a) The trait: no method may return anything.
        let trait_body = between(source, "pub trait LiveSafe {", "\n}\n")
            .expect("the LiveSafe trait declaration");
        for line in trait_body.lines().map(str::trim) {
            if !line.starts_with("fn ") {
                continue;
            }
            assert!(
                !line.contains("->"),
                "LiveSafe methods must return `()` so no caller can be tempted \
                 to handle a failure on the live path: {line}"
            );
        }
        let methods = trait_body
            .lines()
            .filter(|l| l.trim_start().starts_with("fn "))
            .count();
        assert!(methods >= 14, "the trait shrank to {methods} methods");

        // (b) The implementation: no blocking construct anywhere in it.
        let impl_body = between(source, "impl LiveSafe for QualityCollector {", "\n}\n")
            .expect("the LiveSafe impl block");
        for (needle, why) in [
            (".await", "async work suspends the caller"),
            (".lock(", "a mutex can be held by a slow thread"),
            (
                "Result",
                "a fallible call invites handling on the live path",
            ),
            ("std::fs", "the disk is the thing law 1 forbids"),
            ("File::", "the disk is the thing law 1 forbids"),
            (
                "sleep",
                "waiting is never an option between a key and a pixel",
            ),
            ("block_on", "blocking on a runtime is the worst case of all"),
        ] {
            assert!(
                !impl_body.contains(needle),
                "`{needle}` appears in the live-safe impl — {why}"
            );
        }
        // A channel is fine; a BLOCKING one is not. `try_send` drops on a full
        // buffer, `send` waits for a reader — and the reader is a disk.
        assert_eq!(
            impl_body.matches(".send(").count(),
            0,
            "the live-safe impl may only use `try_send`, never a blocking `send`"
        );
        assert!(
            impl_body.contains(".try_send("),
            "the buffer is still bounded"
        );
    }

    /// The substring of `haystack` between the first `start` and the next `end`
    /// after it.
    fn between<'a>(haystack: &'a str, start: &str, end: &str) -> Option<&'a str> {
        let from = haystack.find(start)? + start.len();
        let len = haystack[from..].find(end)?;
        Some(&haystack[from..from + len])
    }

    // ── (2) the live gate ────────────────────────────────────────────────────

    /// A store that refuses to be touched. Injected to prove the gate holds:
    /// the guarantee is "the disk is not reached", and the only way to observe
    /// "not reached" is for the sink to object loudly if it ever is.
    struct PanickingStore {
        touched: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl QualityStore for PanickingStore {
        async fn write_quality(&self, _row: &QualityRow) -> Result<(), String> {
            self.touched.fetch_add(1, Ordering::SeqCst);
            panic!("the store must never be touched while a service is live");
        }
        async fn add_counters(&self, _d: &[(CounterName, u64)]) -> Result<(), String> {
            self.touched.fetch_add(1, Ordering::SeqCst);
            panic!("the store must never be touched while a service is live");
        }
    }

    /// A store that records what it was given.
    #[derive(Default)]
    struct RecordingStore {
        rows: Mutex<Vec<QualityRow>>,
        counters: Mutex<Vec<(CounterName, u64)>>,
        fail: AtomicBool,
    }

    #[async_trait::async_trait]
    impl QualityStore for RecordingStore {
        async fn write_quality(&self, row: &QualityRow) -> Result<(), String> {
            if self.fail.load(Ordering::SeqCst) {
                return Err("disk_full".into());
            }
            self.rows.lock().push(row.clone());
            Ok(())
        }
        async fn add_counters(&self, d: &[(CounterName, u64)]) -> Result<(), String> {
            if self.fail.load(Ordering::SeqCst) {
                return Err("disk_full".into());
            }
            self.counters.lock().extend_from_slice(d);
            Ok(())
        }
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
    }

    #[test]
    fn a_drain_while_live_never_touches_the_store() {
        let rt = runtime();
        let collector = QualityCollector::new();
        collector.begin_session(1_000);
        collector.note_cue_dispatched();
        collector.finish_session(2_000, SessionOutcome::Clean);
        // …and one more session's worth of counters still buffered.
        collector.note_counter(CounterName::OutputOpened);

        let store = PanickingStore {
            touched: std::sync::atomic::AtomicUsize::new(0),
        };
        let live: Mutex<Option<LiveSession>> = Mutex::new(Some(empty_session()));

        // A service is running: the store must not be reached, and the drain
        // must return normally rather than propagating the sink's objection.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(collector.drain_if_quiet(&live, &store))
        }));
        let report = outcome.expect("the drain must not panic while live");
        assert!(report.skipped_live);
        assert_eq!(report.rows_written, 0, "zero writes while live");
        assert_eq!(report.counters_written, 0);
        assert_eq!(
            store.touched.load(Ordering::SeqCst),
            0,
            "the store was not touched at all"
        );

        // POSITIVE CONTROL: with the gate open the very same sink IS reached —
        // so the assertion above is about the gate, not about a drain that
        // never had anything to write.
        let live: Mutex<Option<LiveSession>> = Mutex::new(None);
        let reached = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(collector.drain_if_quiet(&live, &store))
        }));
        assert!(
            reached.is_err(),
            "with no live session the store must be reached"
        );
        assert_eq!(store.touched.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_try_lock_miss_counts_as_live() {
        // A miss means another thread is inside a live command right now, which
        // is the exact moment not to touch a disk. Hold the lock on this thread
        // and prove the gate reads closed even though the session is `None`.
        let live: Mutex<Option<LiveSession>> = Mutex::new(None);
        assert!(gate_is_open(&live), "no session, lock free → open");
        let _guard = live.lock();
        assert!(!gate_is_open(&live), "a try-lock miss must read as live");
    }

    #[test]
    fn draining_twice_writes_once() {
        let rt = runtime();
        let collector = QualityCollector::new();
        let store = RecordingStore::default();
        let live: Mutex<Option<LiveSession>> = Mutex::new(None);

        collector.begin_session(1_000);
        collector.note_cue_dispatched();
        collector.finish_session(61_000, SessionOutcome::Clean);

        let first = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert_eq!(first.rows_written, 1);
        let second = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert_eq!(second.rows_written, 0, "the watermark held");
        assert_eq!(store.rows.lock().len(), 1);
        // The counters were consumed once too.
        assert!(first.counters_written > 0);
        assert_eq!(second.counters_written, 0);
    }

    #[test]
    fn a_failed_write_puts_the_work_back() {
        let rt = runtime();
        let collector = QualityCollector::new();
        let store = RecordingStore::default();
        let live: Mutex<Option<LiveSession>> = Mutex::new(None);
        store.fail.store(true, Ordering::SeqCst);

        collector.begin_session(1_000);
        collector.note_counter(CounterName::OutputOpened);
        collector.finish_session(2_000, SessionOutcome::Clean);

        let failed = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert_eq!(failed.write_failures, 1);
        assert_eq!(failed.rows_written, 0);

        // Nothing was lost: a later pass against a healthy store writes it.
        store.fail.store(false, Ordering::SeqCst);
        let ok = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert_eq!(ok.rows_written, 1);
        assert!(ok.counters_written >= 2, "the counter deltas came back too");
    }

    /// A store that refuses exactly the FIRST row it is shown, then works —
    /// the transient `database is locked` a busy SQLite hands back.
    #[derive(Default)]
    struct RefusesTheFirstRow {
        rows: Mutex<Vec<QualityRow>>,
        seen: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl QualityStore for RefusesTheFirstRow {
        async fn write_quality(&self, row: &QualityRow) -> Result<(), String> {
            if self.seen.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err("database is locked".into());
            }
            self.rows.lock().push(row.clone());
            Ok(())
        }
        async fn add_counters(&self, _d: &[(CounterName, u64)]) -> Result<(), String> {
            Ok(())
        }
    }

    /// The seam an `at` watermark could not survive: a row is put back at the
    /// BACK of the buffer after a failed write, so a NEWER row can be written
    /// in the same pass. A forward-only watermark then read the older row as
    /// "already persisted" and skipped it — for good, without writing it and
    /// without putting it back. One transient sqlite error, and a whole
    /// service's quality row vanished silently.
    #[test]
    fn a_row_requeued_after_a_failed_write_is_never_lost() {
        let rt = runtime();
        let collector = QualityCollector::new();
        let live: Mutex<Option<LiveSession>> = Mutex::new(None);

        // Two finished sessions buffered before any quiet moment: A older than B.
        collector.begin_session(1_000);
        collector.finish_session(100_000, SessionOutcome::Clean); // A
        collector.begin_session(200_000);
        collector.finish_session(300_000, SessionOutcome::Clean); // B

        let store = RefusesTheFirstRow::default();

        // Pass 1: A's INSERT hits the transient error and is re-buffered; B
        // succeeds — and under the old rule pushed the watermark past A.
        let first = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert_eq!(first.write_failures, 1, "A failed as arranged");
        assert_eq!(first.rows_written, 1, "B went in");

        // Pass 2 against a store that is healthy again.
        let second = rt.block_on(collector.drain_if_quiet(&live, &store));

        let ats: Vec<i64> = store.rows.lock().iter().map(|r| r.at).collect();
        assert!(
            ats.contains(&100_000),
            "the re-buffered row must be retried, not skipped. Written: {ats:?}, \
             second pass: {second:?}"
        );
        assert_eq!(second.rows_written, 1, "exactly the retry");

        // …and a third pass writes nothing: retrying is not re-writing.
        let third = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert_eq!(third.rows_written, 0);
        assert_eq!(store.rows.lock().len(), 2, "each row landed exactly once");
    }

    /// Writing a quality row takes the app live, exactly as a `live_start`
    /// landing mid-drain would. Records whether the counters were written anyway.
    struct GoesLiveMidDrain<'a> {
        live: &'a Mutex<Option<LiveSession>>,
        counters_written_while_live: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl QualityStore for GoesLiveMidDrain<'_> {
        async fn write_quality(&self, _row: &QualityRow) -> Result<(), String> {
            *self.live.lock() = Some(empty_session());
            Ok(())
        }
        async fn add_counters(&self, _d: &[(CounterName, u64)]) -> Result<(), String> {
            if self.live.lock().is_some() {
                self.counters_written_while_live
                    .fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }

    /// Law 1 for the COUNTER write. The row loop re-checks the gate between
    /// rows; the counter write below it used to have no re-check at all, so a
    /// service that went live mid-drain still got a disk write.
    #[test]
    fn counters_are_not_written_once_a_service_goes_live_mid_drain() {
        let rt = runtime();
        let collector = QualityCollector::new();
        let live: Mutex<Option<LiveSession>> = Mutex::new(None);

        collector.begin_session(1_000);
        collector.note_cue_dispatched();
        collector.finish_session(2_000, SessionOutcome::Clean);

        let store = GoesLiveMidDrain {
            live: &live,
            counters_written_while_live: std::sync::atomic::AtomicUsize::new(0),
        };

        let report = rt.block_on(collector.drain_if_quiet(&live, &store));

        assert_eq!(
            store.counters_written_while_live.load(Ordering::SeqCst),
            0,
            "the counter write reached SQLite while a service was LIVE"
        );
        assert!(report.skipped_live, "the drain says why it stopped");
        assert_eq!(report.counters_written, 0);

        // Nothing was lost either: with the gate open again the deltas are
        // still there to be written.
        *live.lock() = None;
        let store = RecordingStore::default();
        let after = rt.block_on(collector.drain_if_quiet(&live, &store));
        assert!(
            after.counters_written > 0,
            "the restored deltas reached the next quiet moment"
        );
    }

    #[test]
    fn the_buffer_is_bounded_and_drops_rather_than_waits() {
        let collector = QualityCollector::new();
        for i in 0..(BUFFER_MAX + 5) {
            collector.begin_session(i as i64 * 1_000);
            collector.finish_session(i as i64 * 1_000 + 500, SessionOutcome::Clean);
        }
        assert_eq!(
            collector.dropped_rows(),
            5,
            "the oldest news is dropped, and counted"
        );
    }

    // ── (3) the row's arithmetic ─────────────────────────────────────────────

    #[test]
    fn a_clean_session_passes_with_the_clean_reason() {
        let collector = QualityCollector::new();
        collector.begin_session(1_000);
        for _ in 0..12 {
            collector.note_cue_dispatched();
        }
        collector.finish_session(5_401_000, SessionOutcome::Clean);
        let row = collector.rx.lock().try_recv().expect("a row was buffered");
        assert_eq!(row.verdict, QualityVerdict::Pass);
        assert_eq!(row.reasons, vec![QualityReason::Clean]);
        assert_eq!(row.cue_count, 12);
        assert_eq!(row.duration_sec, 5_400);
        assert!(!row.abnormal_end && !row.recovered);
        assert_eq!(row.cue_latency_p95_ms, None, "nothing was ACKed");
    }

    #[test]
    fn a_restarted_output_child_is_a_warn_not_a_fail() {
        // The E7 rig checklist in the programme: kill the output child
        // mid-service → the restart is counted and the verdict is `warn`,
        // because hold-last-frame covered the gap. That is the whole point of
        // the child being a separate process.
        let collector = QualityCollector::new();
        collector.begin_session(0);
        collector.note_output_restart();
        collector.note_watchdog_hold();
        collector.finish_session(60_000, SessionOutcome::Clean);
        let row = collector.rx.lock().try_recv().expect("row");
        assert_eq!(row.verdict, QualityVerdict::Warn);
        assert_eq!(row.output_child_restarts, 1);
        assert!(row.reasons.contains(&QualityReason::OutputRestarted));
        assert!(row.reasons.contains(&QualityReason::HoldLastFrame));
        assert!(!row.reasons.contains(&QualityReason::Clean));
    }

    #[test]
    fn every_signal_lands_in_its_own_field() {
        let collector = QualityCollector::new();
        collector.begin_session(0);
        collector.note_connect_timeout();
        collector.note_dispatch_error();
        collector.note_dispatch_error();
        collector.note_companion_failure();
        collector.note_fallback_in_process();
        collector.note_stale_child_reaped();
        collector.finish_session(1_000, SessionOutcome::Recovered);
        let row = collector.rx.lock().try_recv().expect("row");
        assert_eq!(row.connect_timeouts, 1);
        assert_eq!(row.dispatch_errors, 2);
        assert_eq!(row.companion_failures, 1);
        assert!(row.fallback_used);
        assert!(row.stale_child_reaped);
        assert!(row.recovered);
        assert_eq!(row.verdict, QualityVerdict::Warn);
        for want in [
            QualityReason::DispatchErrors,
            QualityReason::CompanionFailures,
            QualityReason::FallbackUsed,
        ] {
            assert!(row.reasons.contains(&want), "{want:?} missing: {row:?}");
        }
    }

    /// One service's numbers never belong to another's row.
    ///
    /// A second `live_start` without a `live_end` between them is an ordinary
    /// morning: the operator rehearses at 09:40 and goes live at 11:00, or
    /// switches to the second service from the schedule. `begin_session` used
    /// to reset only `started_at`, the recovered flag and the latency
    /// histogram — so the real service's row carried the rehearsal's cues,
    /// restarts and failures, which is enough to turn a clean Sunday into a
    /// `warn` with reasons that describe a service nobody ran.
    #[test]
    fn begin_session_starts_every_counter_from_zero() {
        let collector = QualityCollector::new();

        // Service A goes live and the operator drives it…
        collector.begin_session(1_000);
        for _ in 0..40 {
            collector.note_cue_dispatched();
        }
        collector.note_output_restart();
        collector.note_connect_timeout();
        collector.note_watchdog_hold();
        collector.note_dispatch_error();
        collector.note_companion_failure();

        // …then a second `live_start` lands without a `live_end` in between.
        collector.begin_session(500_000);
        collector.note_cue_dispatched();
        collector.finish_session(600_000, SessionOutcome::Clean);

        let row = collector
            .take_buffered_rows()
            .pop()
            .expect("service B's row");
        assert_eq!(row.cue_count, 1, "A's 40 cues leaked into B: {row:?}");
        assert_eq!(row.output_child_restarts, 0, "A's restart leaked into B");
        assert_eq!(row.connect_timeouts, 0, "A's connect timeout leaked into B");
        assert_eq!(row.watchdog_holds, 0, "A's held frame leaked into B");
        assert_eq!(row.dispatch_errors, 0, "A's dispatch error leaked into B");
        assert_eq!(row.companion_failures, 0, "A's companion failure leaked");
        assert_eq!(row.duration_sec, 100, "B's own clock");
        assert_eq!(
            row.verdict,
            QualityVerdict::Pass,
            "B ran clean and its verdict says so: {row:?}"
        );
    }

    #[test]
    fn the_fallback_flag_is_sticky_until_isolation_returns() {
        let collector = QualityCollector::new();
        collector.note_fallback_in_process();
        collector.begin_session(0);
        collector.finish_session(1_000, SessionOutcome::Clean);
        assert!(
            collector.rx.lock().try_recv().expect("row").fallback_used,
            "how the outputs are RUNNING outlives one service"
        );
        // Reopening with isolation clears it.
        collector.note_isolated_outputs();
        collector.begin_session(2_000);
        collector.finish_session(3_000, SessionOutcome::Clean);
        assert!(!collector.rx.lock().try_recv().expect("row").fallback_used);
    }

    #[test]
    fn counters_are_bumped_by_the_session_lifecycle() {
        let collector = QualityCollector::new();
        collector.begin_session(0);
        collector.note_cue_dispatched();
        collector.note_cue_dispatched();
        collector.finish_session(1_000, SessionOutcome::Clean);
        let got = collector.counters().peek();
        assert!(got.contains(&(CounterName::LiveSessionStarted, 1)));
        assert!(got.contains(&(CounterName::LiveCueDispatched, 2)));
        assert!(got.contains(&(CounterName::LiveSessionEnded, 1)));
        // A recovered session is counted as recovered, not as a clean end —
        // and `live_end` says `Clean` for every session, so the RESUME has to
        // be remembered from when the session began.
        collector.begin_session(2_000);
        collector.note_session_recovered();
        collector.finish_session(3_000, SessionOutcome::Clean);
        let got = collector.counters().peek();
        assert!(got.contains(&(CounterName::LiveSessionRecovered, 1)));
        assert!(
            got.contains(&(CounterName::LiveSessionEnded, 1)),
            "still one clean end"
        );
        let rows = collector.take_buffered_rows();
        assert!(!rows[0].recovered, "the first session was not a resume");
        assert!(rows[1].recovered, "the second was");
        // …and the flag does not leak into the session after it.
        collector.begin_session(4_000);
        collector.finish_session(5_000, SessionOutcome::Clean);
        assert!(!collector.take_buffered_rows()[0].recovered);
    }

    #[test]
    fn the_verdict_rules_are_exactly_the_programmes() {
        let base = QualityRow {
            id: "x".into(),
            dedupe_key: None,
            at: 0,
            duration_sec: 0,
            cue_count: 0,
            output_child_restarts: 0,
            connect_timeouts: 0,
            watchdog_holds: 0,
            dispatch_errors: 0,
            companion_failures: 0,
            fallback_used: false,
            stale_child_reaped: false,
            abnormal_end: false,
            recovered: false,
            cue_latency_p95_ms: None,
            verdict: QualityVerdict::Pass,
            reasons: vec![],
        };
        assert_eq!(
            judge(&base),
            (QualityVerdict::Pass, vec![QualityReason::Clean])
        );
        // Abnormal end is the ONLY fail.
        let mut abnormal = base.clone();
        abnormal.abnormal_end = true;
        assert_eq!(judge(&abnormal).0, QualityVerdict::Fail);
        // The cue budget: at the budget is fine, past it is `slow-cues`.
        let mut at_budget = base.clone();
        at_budget.cue_latency_p95_ms = Some(CUE_BUDGET_MS);
        assert_eq!(judge(&at_budget).0, QualityVerdict::Pass);
        let mut over = base.clone();
        over.cue_latency_p95_ms = Some(CUE_BUDGET_MS + 1);
        assert_eq!(
            judge(&over),
            (QualityVerdict::Warn, vec![QualityReason::SlowCues])
        );
        // A connect timeout alone is recorded but is not, by itself, a reason —
        // the restart it causes is what the operator would have seen.
        let mut timeout = base.clone();
        timeout.connect_timeouts = 3;
        assert_eq!(judge(&timeout).1, vec![QualityReason::Clean]);
        // `reasons` is never empty, and every code round-trips its wire string.
        for reason in judge(&over).1 {
            assert_eq!(QualityReason::from_wire(reason.as_str()), Some(reason));
        }
        for v in [
            QualityVerdict::Pass,
            QualityVerdict::Warn,
            QualityVerdict::Fail,
        ] {
            assert_eq!(QualityVerdict::from_wire(v.as_str()), Some(v));
        }
        assert_eq!(QualityVerdict::from_wire("great"), None);
        assert_eq!(QualityReason::from_wire("everything-broke"), None);
    }

    // ── (4) cue latency ──────────────────────────────────────────────────────

    #[test]
    fn deltas_land_in_the_right_bucket() {
        assert_eq!(bucket_of(0), 0);
        assert_eq!(bucket_of(10), 0);
        assert_eq!(bucket_of(11), 1);
        assert_eq!(bucket_of(25), 1);
        assert_eq!(bucket_of(50), 2);
        assert_eq!(bucket_of(51), 3);
        assert_eq!(bucket_of(250), 4);
        assert_eq!(bucket_of(251), 5);
        assert_eq!(bucket_of(u64::MAX), 5);
    }

    #[test]
    fn p95_interpolates_inside_the_bucket_it_lands_in() {
        assert_eq!(p95_from_buckets(&[0; LATENCY_BUCKETS]), None);
        // Everything fast: the p95 sits inside the first bucket, at its top.
        assert_eq!(p95_from_buckets(&[100, 0, 0, 0, 0, 0]), Some(10));
        // 95 fast, 5 slow: the 95th of 100 is rank 95, still the first bucket.
        assert_eq!(p95_from_buckets(&[95, 5, 0, 0, 0, 0]), Some(10));
        // 90 fast, 10 in 10–25 ms: rank 95 is the 5th of that bucket → halfway.
        assert_eq!(p95_from_buckets(&[90, 10, 0, 0, 0, 0]), Some(18));
        // A single sample IS the p95.
        assert_eq!(p95_from_buckets(&[0, 0, 1, 0, 0, 0]), Some(50));
        // The overflow bucket saturates at its lower edge, honestly.
        assert_eq!(p95_from_buckets(&[0, 0, 0, 0, 0, 3]), Some(250));
    }

    #[test]
    fn latency_is_seq_correlated_and_measured_once_per_cue() {
        let l = CueLatency::new();
        l.dispatched(1);
        l.acked(1);
        assert_eq!(l.snapshot().iter().sum::<u64>(), 1);
        // A second screen ACKing the same seq is the same keypress, not a
        // second data point.
        l.acked(1);
        assert_eq!(l.snapshot().iter().sum::<u64>(), 1);
        // An ACK for a seq that was never dispatched is ignored, not guessed.
        l.acked(999);
        assert_eq!(l.snapshot().iter().sum::<u64>(), 1);
        // seq 0 never happens (the supervisor starts at 1) and is inert.
        l.dispatched(0);
        l.acked(0);
        assert_eq!(l.snapshot().iter().sum::<u64>(), 1);
    }

    #[test]
    fn a_wrapped_slot_drops_the_measurement_rather_than_mis_attributing_it() {
        let l = CueLatency::new();
        l.dispatched(1);
        // SEQ_SLOTS renders later the slot belongs to a different cue.
        l.dispatched(1 + SEQ_SLOTS as u64);
        l.acked(1);
        assert_eq!(
            l.snapshot().iter().sum::<u64>(),
            0,
            "a stale ACK must not be attributed to the newer cue"
        );
        // …and the newer cue still measures correctly.
        l.acked(1 + SEQ_SLOTS as u64);
        assert_eq!(l.snapshot().iter().sum::<u64>(), 1);
    }

    #[test]
    fn a_measured_cue_reaches_the_row() {
        let collector = QualityCollector::new();
        collector.begin_session(0);
        collector.note_cue_dispatched();
        collector.note_render(1);
        collector.note_ack(1);
        collector.finish_session(1_000, SessionOutcome::Clean);
        let row = collector.rx.lock().try_recv().expect("row");
        assert!(
            row.cue_latency_p95_ms.is_some(),
            "a measured ACK must produce a p95"
        );
        // In a unit test the round trip is microseconds — bucket 0.
        assert!(row.cue_latency_p95_ms.unwrap_or(u32::MAX) <= 10);
        // …and the histogram is reset for the next service.
        collector.begin_session(2_000);
        collector.finish_session(3_000, SessionOutcome::Clean);
        assert_eq!(
            collector
                .rx
                .lock()
                .try_recv()
                .expect("row")
                .cue_latency_p95_ms,
            None
        );
    }

    // ── (5) startup reconstruction ───────────────────────────────────────────

    /// A session that started an hour before the crash, with a WAL written
    /// (and so an mtime) NOW — the shape every real leftover has.
    fn a_wal_from_an_hour_long_service(dir: &Path) -> crate::services::session_store::SessionStore {
        let store = crate::services::session_store::SessionStore::in_dir(dir);
        let mut session = LiveSession::new(
            "svc",
            CueList {
                service_id: "svc".into(),
                compiled_at: 0,
                cues: vec![],
            },
            crate::db::now_ms() - 3_600_000,
        );
        store.begin(&session).expect("begin");
        for action in [LiveAction::Next, LiveAction::Blackout] {
            session.dispatch(action.clone(), 2_000);
            store.record(&action).expect("record");
        }
        store
    }

    #[test]
    fn a_leftover_wal_reconstructs_an_abnormal_end_for_the_previous_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            reconstruct_previous_session(dir.path(), 10_000).is_none(),
            "a clean shutdown leaves nothing to reconstruct"
        );

        let store = a_wal_from_an_hour_long_service(dir.path());
        let now = crate::db::now_ms();

        let row =
            reconstruct_previous_session(dir.path(), now).expect("the leftover WAL is a finding");
        assert!(row.abnormal_end);
        assert_eq!(row.verdict, QualityVerdict::Fail);
        assert!(row.reasons.contains(&QualityReason::AbnormalEnd));
        assert_eq!(row.cue_count, 2, "the WAL's action count");
        assert!(
            (3_590..=3_610).contains(&row.duration_sec),
            "the service ran an hour: {}",
            row.duration_sec
        );
        assert!(!row.recovered, "reconstruction is not a resume");
        // Stable across launches, so `INSERT OR IGNORE` writes it once even if
        // the operator never resumes.
        let again = reconstruct_previous_session(dir.path(), now + 1_000).expect("still there");
        assert_eq!(again.dedupe_key, row.dedupe_key);
        assert_ne!(again.id, row.id, "each attempt still mints its own id");

        // …and it is gone once the session is ended cleanly.
        store.clear();
        assert!(reconstruct_previous_session(dir.path(), now + 1_000).is_none());
    }

    /// The machine was OFF between the crash and the next launch, and a
    /// reconstructed duration must not count that time as service.
    ///
    /// The report this protects is the one a support conversation reads: a
    /// crash on Sunday morning and a relaunch the following Saturday used to
    /// produce a six-day session — long enough to look like a hung projector
    /// nobody noticed, in a row whose verdict is already `fail`.
    #[test]
    fn a_reconstructed_duration_does_not_count_the_time_the_machine_was_off() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _store = a_wal_from_an_hour_long_service(dir.path());

        let six_days_later = crate::db::now_ms() + 6 * 24 * 3_600_000;
        let row = reconstruct_previous_session(dir.path(), six_days_later).expect("a finding");
        assert!(
            (3_590..=3_610).contains(&row.duration_sec),
            "the service lasted an hour; the laptop was shut until Saturday. \
             Reported duration: {} s",
            row.duration_sec
        );
        // The row is still dated when it was OBSERVED — that part was never a
        // guess, and E5's send window depends on it.
        assert_eq!(row.at, six_days_later);
    }

    /// No readable mtime — say nothing rather than invent a number.
    #[test]
    fn a_reconstructed_duration_is_zero_when_the_end_cannot_be_known() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = crate::services::session_store::SessionStore::in_dir(dir.path());
        let session = empty_session(); // started_at = 1_000, i.e. 1970
        store.begin(&session).expect("begin");
        // The WAL's mtime is NOW, which is not `<= at_ms` here: the end bound is
        // outside the session's window, so it is not usable at all.
        let row = reconstruct_previous_session(dir.path(), 601_000).expect("a finding");
        assert_eq!(
            row.duration_sec, 0,
            "an unknowable duration is 0, never a made-up span"
        );
    }
}
