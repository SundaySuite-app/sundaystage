//! E5 — the telemetry outbox state machine. Pure: no persistence, no timers.
//!
//! The transitions over an in-memory `Vec<TelemetryEntry>` live here and carry
//! the tests, while [`crate::db::repositories::TelemetryRepo`] owns the SQLite
//! table and [`super::sender`] owns the clock and the socket. Every function
//! takes `now_ms` rather than reading a clock.
//!
//! ## Why a telemetry queue is not a backup queue
//!
//! Three properties follow from what telemetry IS, and each is the opposite of
//! what a "don't lose the user's file" queue would do:
//!
//!   - **A blocked send is nobody's problem.** [`MAX_ATTEMPTS`] is 6 and the
//!     ladder reaches a day quickly, because the data describes something that
//!     already happened and will read the same tomorrow.
//!   - **The queue is bounded, and the bound drops the OLDEST.** A laptop that
//!     spends a winter offline must not accumulate a megabyte of stale reports
//!     about a version nobody runs any more.
//!   - **There is nothing to re-authenticate.** Telemetry is anonymous.
//!
//! ## The consent gate lives here
//!
//! [`pump_decision`] is the only function a sender loop calls to find out what
//! to do, and its FIRST argument is whether consent is active. With consent off
//! it returns [`PumpDecision::Blocked`] without so much as reading the queue —
//! so "no consent means no network activity, not even DNS" is a property of a
//! pure function with a test, rather than a discipline someone has to remember
//! at every call site.
//!
//! ## Divergence from SundayRec
//!
//! One: [`on_failure`] takes the endpoint's `Retry-After`. Rec's ladder ignores
//! it, which is safe but impolite — a 429 that names a window is the endpoint
//! telling us exactly when it will accept us, and a client that ignores it is
//! guessing at something it was told. The delay is `max(ladder, retry_after)`,
//! so honouring the header can only ever make the client wait LONGER, never
//! hammer sooner than the ladder would have.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Give up on a payload after this many attempts. Six: a report that six
/// spread-out attempts could not deliver is describing a version and a machine
/// state that have moved on.
pub const MAX_ATTEMPTS: u32 = 6;

/// Per-attempt backoff after a failure, indexed by `attempts - 1` and clamped to
/// the last step: 1 min → 5 → 15 → 1 h → 6 h → 24 h.
///
/// Front-loaded (a flaky minute of church wifi resolves on the second try) but
/// it reaches a day quickly, because there is no urgency.
pub const BACKOFF_STEPS_MS: [i64; 6] = [
    60_000,
    5 * 60_000,
    15 * 60_000,
    60 * 60_000,
    6 * 60 * 60_000,
    24 * 60 * 60_000,
];

/// The longest a `Retry-After` may park a row, whatever the endpoint says.
///
/// A header is a number from somewhere else. A typo, a misconfigured proxy or a
/// hostile intermediary saying `Retry-After: 99999999` must not silently retire
/// a payload for a decade — the ladder's own ceiling is the natural cap.
pub const MAX_RETRY_AFTER_MS: i64 = 24 * 60 * 60_000;

/// How many payloads the outbox may hold before the oldest are dropped.
///
/// Fifty, and the reasoning so it can be re-argued rather than guessed at again:
/// a payload is bounded by the caps in [`super::payload`] — 20 crash records
/// (≤ ~350 bytes each), 20 quality rows (~300 bytes each), 19 counters, a
/// 10-field settings block — so a FULL one is roughly 15 kB and a typical one is
/// well under 2 kB. Fifty full payloads is ~750 kB worst case, the same order as
/// the crash ring's ceiling and nothing beside a single media asset.
///
/// Fifty is also far more than any real backlog. Payloads are enqueued when
/// something happened, not on a timer, so a church holding one service a week
/// fills fifty rows in a year of being offline; a machine crashing constantly
/// fills them in days, and by then the fifty NEWEST describe the problem better
/// than the fifty oldest would. Dropping from the front is what keeps that true.
pub const QUEUE_MAX: usize = 50;

/// Where a queued payload is in its lifecycle. Serialised lowercase, matching
/// the `status` CHECK constraint in migration 0008.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryStatus.ts")]
#[serde(rename_all = "lowercase")]
pub enum TelemetryStatus {
    /// Waiting for its `next_attempt` time to pass.
    Pending,
    /// Currently being sent.
    Sending,
    /// Out of attempts. Kept only so the settings panel can say so; the
    /// [`QUEUE_MAX`] bound reclaims it.
    Failed,
}

impl TelemetryStatus {
    /// The stored spelling — the same characters the CHECK constraint lists.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::Failed => "failed",
        }
    }

    /// Parse a stored status. An unknown value reads as `Pending` — the row is
    /// retried rather than stranded, which is the recoverable direction.
    pub fn from_wire(raw: &str) -> Self {
        match raw {
            "sending" => Self::Sending,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// One queued payload.
///
/// `payload_json` is the serialised [`super::payload::StagePayload`] — already
/// scrubbed, already capped. Storing the rendered JSON rather than the struct
/// means a row queued by version N is sent unchanged by version N+1: the bytes
/// the operator could inspect in the preview are the bytes that leave, even
/// across an update that changed the builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEntry {
    pub id: String,
    /// Unix ms (UTC) the payload was built and queued.
    pub created_at: i64,
    /// The [`super::payload::PAYLOAD_SCHEMA`] the payload was built at.
    pub schema_ver: u32,
    /// What this payload is about (`crash:<ts>`, `quality:<ts>`,
    /// `counters:<ts>`). Unique in storage, so the same batch cannot be queued
    /// twice by two racing drains.
    pub dedup_key: String,
    pub payload_json: String,
    pub attempts: u32,
    /// Unix ms — earliest the sender may try this entry.
    pub next_attempt: i64,
    pub last_error: Option<String>,
    pub status: TelemetryStatus,
}

/// What a sender loop should do right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PumpDecision {
    /// Consent is not active. Do nothing — do not read the queue, do not resolve
    /// a hostname, do not open a socket. The ONLY correct behaviour, and the one
    /// this enum exists to make impossible to skip.
    Blocked,
    /// Consent is active but nothing is due.
    Idle,
    /// Send this entry.
    Send(String),
}

/// THE gate. Consent first, everything else after.
///
/// Written as one function returning a three-way decision, rather than a
/// `should_send` boolean beside a `select_next`, because the boolean version has
/// a failure mode: a caller can select the next entry, then build a client, then
/// check consent — and on some stacks a DNS resolution has already been kicked
/// off by the time the check runs. Here there is nothing to select until consent
/// has been confirmed.
pub fn pump_decision(
    consent_active: bool,
    entries: &[TelemetryEntry],
    now_ms: i64,
) -> PumpDecision {
    if !consent_active {
        return PumpDecision::Blocked;
    }
    match select_next(entries, now_ms) {
        Some(id) => PumpDecision::Send(id),
        None => PumpDecision::Idle,
    }
}

/// The id of the next entry to send: the `pending` entry whose `next_attempt`
/// has passed, earliest first. `None` when nothing is runnable.
///
/// Deliberately NOT public — [`pump_decision`] is the entry point, so the
/// consent check cannot be forgotten.
fn select_next(entries: &[TelemetryEntry], now_ms: i64) -> Option<String> {
    entries
        .iter()
        .filter(|e| e.status == TelemetryStatus::Pending && e.next_attempt <= now_ms)
        .min_by_key(|e| (e.next_attempt, e.created_at))
        .map(|e| e.id.clone())
}

/// Reset any entry left in `Sending` back to `Pending`, returning how many.
///
/// Call ONCE at startup: an entry only reaches `Sending` while a send is in
/// flight, so at boot every one of them is stale by definition. Without this a
/// force-quit mid-send strands the row forever.
pub fn reset_stale_sending(entries: &mut [TelemetryEntry]) -> usize {
    let mut reset = 0;
    for e in entries.iter_mut() {
        if e.status == TelemetryStatus::Sending {
            e.status = TelemetryStatus::Pending;
            reset += 1;
        }
    }
    reset
}

/// Transition an entry to `Sending` and count the attempt, just before the send.
pub fn mark_sending(entries: &mut [TelemetryEntry], id: &str) {
    if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
        e.status = TelemetryStatus::Sending;
        e.attempts += 1;
    }
}

/// A delivered payload leaves the queue. Returns `true` if one was removed.
pub fn on_success(entries: &mut Vec<TelemetryEntry>, id: &str) -> bool {
    let before = entries.len();
    entries.retain(|e| e.id != id);
    entries.len() != before
}

/// Apply a failed attempt: back off, or give up at [`MAX_ATTEMPTS`].
///
/// `error` is stored for the settings panel. Attempts are incremented by
/// [`mark_sending`] BEFORE the attempt, so by the time this runs `attempts`
/// already counts this try.
///
/// `retry_after_ms` is the endpoint's `Retry-After`, when it sent one. The wait
/// becomes `max(ladder step, retry_after)` — honouring the header can only make
/// the client wait longer, never sooner than the ladder would have, and a
/// nonsense value is clamped to [`MAX_RETRY_AFTER_MS`].
pub fn on_failure(
    entries: &mut [TelemetryEntry],
    id: &str,
    error: impl Into<String>,
    now_ms: i64,
    retry_after_ms: Option<i64>,
) {
    if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
        e.last_error = Some(error.into());
        if e.attempts >= MAX_ATTEMPTS {
            e.status = TelemetryStatus::Failed;
        } else {
            e.status = TelemetryStatus::Pending;
            let idx = (e.attempts.saturating_sub(1) as usize).min(BACKOFF_STEPS_MS.len() - 1);
            let ladder = BACKOFF_STEPS_MS[idx];
            let asked = retry_after_ms.unwrap_or(0).clamp(0, MAX_RETRY_AFTER_MS);
            e.next_attempt = now_ms + ladder.max(asked);
        }
    }
}

/// Drop an entry the endpoint will never accept. Returns `true` if one went.
///
/// The counterpart to [`on_failure`], and the distinction is the whole reason
/// this function exists rather than being a flag on that one. The backoff ladder
/// answers "the church wifi is down": wait, try again, the same bytes will
/// succeed later. It is exactly wrong for "this payload is malformed", because a
/// malformed payload is malformed the same way all six times — six spread-out
/// attempts over 24 hours, six identical rejections, and a row that then sits in
/// `Failed` until the [`QUEUE_MAX`] bound reclaims it.
///
/// So a permanent rejection removes the row immediately. The endpoint's half of
/// this contract is `sunday-telemetry`'s ingest route; the mapping from status
/// to decision is [`super::http_sender::classify`]. Nothing is kept for the
/// settings panel here, unlike an exhausted ladder: a row that ran out of
/// attempts describes something the operator might recognise (a network that has
/// been down for a day), while a schema rejection describes a disagreement
/// between this build and the endpoint, which they can neither cause nor fix.
/// **It is never silent, though** — [`super::sender::pump_once`] logs the reason
/// locally before the row goes.
pub fn on_permanent_failure(entries: &mut Vec<TelemetryEntry>, id: &str) -> bool {
    let before = entries.len();
    entries.retain(|e| e.id != id);
    entries.len() != before
}

/// Which entry ids must go so at most `cap` remain, oldest first.
///
/// Ordered by `created_at` (then `id`, so the answer is deterministic when two
/// payloads were built in the same millisecond). Failed rows are not preferred
/// over pending ones: a `Failed` row that is NEWER describes a more current
/// problem than a `Pending` row from last winter.
pub fn overflow_victims(entries: &[TelemetryEntry], cap: usize) -> Vec<String> {
    if entries.len() <= cap {
        return Vec::new();
    }
    let mut by_age: Vec<&TelemetryEntry> = entries.iter().collect();
    by_age.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    by_age[..entries.len() - cap]
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

/// A compact status line for the settings panel: how many are waiting, how old
/// the oldest is, and what went wrong last.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryQueueStatus.ts")]
#[serde(rename_all = "camelCase")]
pub struct TelemetryQueueStatus {
    /// Rows not yet delivered (pending + sending).
    #[ts(type = "number")]
    pub pending: u32,
    /// Rows that ran out of attempts.
    #[ts(type = "number")]
    pub failed: u32,
    /// Unix ms (UTC) the oldest undelivered payload was built.
    #[ts(type = "number | null")]
    pub oldest_at: Option<i64>,
    /// The most recent failure message, if any.
    pub last_error: Option<String>,
}

/// Summarise the queue for the UI.
pub fn queue_status(entries: &[TelemetryEntry]) -> TelemetryQueueStatus {
    TelemetryQueueStatus {
        pending: entries
            .iter()
            .filter(|e| e.status != TelemetryStatus::Failed)
            .count() as u32,
        failed: entries
            .iter()
            .filter(|e| e.status == TelemetryStatus::Failed)
            .count() as u32,
        oldest_at: entries
            .iter()
            .filter(|e| e.status != TelemetryStatus::Failed)
            .map(|e| e.created_at)
            .min(),
        // The newest failure is the useful one — an error from three weeks ago
        // describes a network that has since come back.
        last_error: entries
            .iter()
            .filter(|e| e.last_error.is_some())
            .max_by_key(|e| e.created_at)
            .and_then(|e| e.last_error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, created_at: i64) -> TelemetryEntry {
        TelemetryEntry {
            id: id.to_string(),
            created_at,
            schema_ver: 1,
            dedup_key: format!("quality:{created_at}"),
            payload_json: "{}".to_string(),
            attempts: 0,
            next_attempt: created_at,
            last_error: None,
            status: TelemetryStatus::Pending,
        }
    }

    // ── The consent gate ─────────────────────────────────────────────────────

    #[test]
    fn without_consent_the_pump_never_selects_anything() {
        // THE guarantee, as a pure property: whatever the queue holds and
        // whatever the clock says, consent-off yields Blocked. There is no
        // combination of inputs that produces a Send.
        let queues: Vec<Vec<TelemetryEntry>> = vec![
            vec![],
            vec![entry("a", 0)],
            vec![entry("a", 0), entry("b", 1)],
            {
                let mut e = entry("c", 0);
                e.status = TelemetryStatus::Sending;
                vec![e]
            },
            {
                let mut e = entry("d", 0);
                e.status = TelemetryStatus::Failed;
                vec![e]
            },
        ];
        for q in &queues {
            for now in [i64::MIN, -1, 0, 1, 1_800_000_000_000, i64::MAX] {
                assert_eq!(
                    pump_decision(false, q, now),
                    PumpDecision::Blocked,
                    "consent off must block regardless of queue {q:?} / now {now}"
                );
            }
        }
        // …and the positive control, so the assertion above is not vacuous: the
        // SAME queue with consent on does select an entry.
        assert_eq!(
            pump_decision(true, &queues[1], 1_800_000_000_000),
            PumpDecision::Send("a".into())
        );
    }

    #[test]
    fn with_consent_the_pump_is_idle_until_something_is_due() {
        let q = vec![entry("a", 10_000)];
        assert_eq!(pump_decision(true, &q, 9_999), PumpDecision::Idle);
        assert_eq!(
            pump_decision(true, &q, 10_000),
            PumpDecision::Send("a".into())
        );
        assert_eq!(pump_decision(true, &[], 10_000), PumpDecision::Idle);
    }

    #[test]
    fn the_pump_picks_the_earliest_due_entry() {
        let mut later = entry("later", 5_000);
        later.next_attempt = 5_000;
        let mut earlier = entry("earlier", 9_000);
        earlier.next_attempt = 1_000;
        let q = vec![later, earlier];
        assert_eq!(
            pump_decision(true, &q, 10_000),
            PumpDecision::Send("earlier".into()),
            "next_attempt orders the queue, not insertion order"
        );
    }

    #[test]
    fn only_pending_entries_are_selected() {
        for status in [TelemetryStatus::Sending, TelemetryStatus::Failed] {
            let mut e = entry("a", 0);
            e.status = status;
            assert_eq!(
                pump_decision(true, &[e], 1_000_000),
                PumpDecision::Idle,
                "{status:?} must not be picked up"
            );
        }
    }

    // ── The lifecycle ────────────────────────────────────────────────────────

    #[test]
    fn a_send_counts_its_attempt_before_it_happens() {
        let mut q = vec![entry("a", 0)];
        mark_sending(&mut q, "a");
        assert_eq!(q[0].status, TelemetryStatus::Sending);
        assert_eq!(q[0].attempts, 1);
        // An unknown id is a no-op, not a panic.
        mark_sending(&mut q, "ghost");
        assert_eq!(q[0].attempts, 1);
    }

    #[test]
    fn success_removes_the_entry() {
        let mut q = vec![entry("a", 0), entry("b", 1)];
        assert!(on_success(&mut q, "a"));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, "b");
        assert!(!on_success(&mut q, "ghost"));
    }

    #[test]
    fn failures_walk_the_backoff_ladder_then_give_up() {
        let mut q = vec![entry("a", 0)];
        let now = 1_000_000i64;
        for attempt in 1..=MAX_ATTEMPTS {
            mark_sending(&mut q, "a");
            on_failure(&mut q, "a", format!("boom {attempt}"), now, None);
            if attempt < MAX_ATTEMPTS {
                assert_eq!(q[0].status, TelemetryStatus::Pending, "attempt {attempt}");
                assert_eq!(
                    q[0].next_attempt,
                    now + BACKOFF_STEPS_MS[(attempt - 1) as usize],
                    "attempt {attempt} must use the matching ladder step"
                );
            }
        }
        assert_eq!(q[0].status, TelemetryStatus::Failed);
        assert_eq!(q[0].attempts, MAX_ATTEMPTS);
        assert_eq!(q[0].last_error.as_deref(), Some("boom 6"));
        // A failed entry is never selected again, even far in the future.
        assert_eq!(pump_decision(true, &q, i64::MAX), PumpDecision::Idle);
    }

    #[test]
    fn the_ladder_is_the_whole_ladder_and_matches_the_attempt_budget() {
        assert_eq!(BACKOFF_STEPS_MS.len(), MAX_ATTEMPTS as usize);
        assert_eq!(
            BACKOFF_STEPS_MS,
            [
                60_000,     // 1 min
                300_000,    // 5 min
                900_000,    // 15 min
                3_600_000,  // 1 h
                21_600_000, // 6 h
                86_400_000, // 24 h
            ],
            "the programme's ladder, spelled out so a refactor cannot slide it"
        );
    }

    #[test]
    fn the_ladder_is_clamped_rather_than_indexed_out_of_bounds() {
        let mut q = vec![entry("a", 0)];
        // A row that somehow carries more attempts than the ladder has steps (a
        // hand-edited database, a changed MAX_ATTEMPTS) must not panic.
        q[0].attempts = 3;
        on_failure(&mut q, "a", "x", 0, None);
        assert_eq!(q[0].next_attempt, BACKOFF_STEPS_MS[2]);
    }

    #[test]
    fn a_retry_after_longer_than_the_ladder_is_honoured() {
        // The endpoint knows when it will accept us; the ladder is a guess.
        let mut q = vec![entry("a", 0)];
        mark_sending(&mut q, "a");
        on_failure(&mut q, "a", "429", 1_000, Some(10 * 60_000));
        assert_eq!(
            q[0].next_attempt,
            1_000 + 10 * 60_000,
            "a 10-minute Retry-After must beat the 1-minute first rung"
        );
    }

    #[test]
    fn a_retry_after_shorter_than_the_ladder_never_hammers() {
        let mut q = vec![entry("a", 0)];
        mark_sending(&mut q, "a");
        on_failure(&mut q, "a", "429", 1_000, Some(2_000));
        assert_eq!(
            q[0].next_attempt,
            1_000 + BACKOFF_STEPS_MS[0],
            "honouring a header may lengthen the wait, never shorten it"
        );
    }

    #[test]
    fn a_nonsense_retry_after_is_clamped_rather_than_believed() {
        // A header is a number from somewhere else: a typo or a broken proxy
        // must not retire a payload for a decade, and a negative one must not
        // schedule it in the past.
        let mut q = vec![entry("a", 0)];
        mark_sending(&mut q, "a");
        on_failure(&mut q, "a", "429", 0, Some(i64::MAX));
        assert_eq!(q[0].next_attempt, MAX_RETRY_AFTER_MS);

        let mut q = vec![entry("b", 0)];
        mark_sending(&mut q, "b");
        on_failure(&mut q, "b", "429", 5_000, Some(-99));
        assert_eq!(q[0].next_attempt, 5_000 + BACKOFF_STEPS_MS[0]);
    }

    #[test]
    fn a_force_quit_mid_send_is_recovered_at_startup() {
        let mut q = vec![entry("a", 0), entry("b", 1)];
        q[0].status = TelemetryStatus::Sending;
        assert_eq!(reset_stale_sending(&mut q), 1);
        assert_eq!(q[0].status, TelemetryStatus::Pending);
        // Idempotent.
        assert_eq!(reset_stale_sending(&mut q), 0);
    }

    // ── The bound ────────────────────────────────────────────────────────────

    #[test]
    fn the_queue_bound_drops_the_oldest() {
        let q: Vec<TelemetryEntry> = (0..QUEUE_MAX + 7)
            .map(|i| entry(&format!("id-{i:03}"), i as i64))
            .collect();
        let victims = overflow_victims(&q, QUEUE_MAX);
        assert_eq!(victims.len(), 7);
        assert_eq!(victims[0], "id-000", "oldest goes first");
        assert_eq!(victims[6], "id-006");
        // Under the cap nothing is dropped.
        assert!(overflow_victims(&q[..QUEUE_MAX], QUEUE_MAX).is_empty());
        assert!(overflow_victims(&[], QUEUE_MAX).is_empty());
        assert_eq!(QUEUE_MAX, 50);
    }

    #[test]
    fn the_bound_is_deterministic_when_payloads_share_a_millisecond() {
        let q = vec![entry("b", 5), entry("a", 5), entry("c", 9)];
        assert_eq!(
            overflow_victims(&q, 1),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn a_newer_failed_row_outlives_an_older_pending_one() {
        // Relevance beats status: a failed report about today's version is
        // better company than a pending one about last winter's.
        let mut old_pending = entry("old", 1);
        old_pending.status = TelemetryStatus::Pending;
        let mut new_failed = entry("new", 100);
        new_failed.status = TelemetryStatus::Failed;
        let victims = overflow_victims(&[old_pending, new_failed], 1);
        assert_eq!(victims, vec!["old".to_string()]);
    }

    // ── Permanent rejection ──────────────────────────────────────────────────

    #[test]
    fn a_permanent_failure_drops_the_row_instead_of_backing_off() {
        let mut q = vec![entry("a", 0), entry("b", 1)];
        assert!(on_permanent_failure(&mut q, "a"));
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, "b");
        // Idempotent: a second permanent failure for the same id is a no-op.
        assert!(!on_permanent_failure(&mut q, "a"));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn a_permanent_failure_leaves_no_attempt_state_behind() {
        // Contrast with `on_failure`, which is the whole point of having both.
        let now = 1_800_000_000_000i64;
        let mut transient = vec![entry("a", 0)];
        mark_sending(&mut transient, "a");
        on_failure(&mut transient, "a", "connection reset", now, None);
        assert_eq!(transient.len(), 1, "a transient failure keeps the payload");
        assert_eq!(transient[0].attempts, 1);
        assert_eq!(transient[0].next_attempt, now + BACKOFF_STEPS_MS[0]);

        let mut permanent = vec![entry("a", 0)];
        mark_sending(&mut permanent, "a");
        on_permanent_failure(&mut permanent, "a");
        assert!(permanent.is_empty(), "a permanent failure keeps nothing");
    }

    #[test]
    fn a_dropped_payload_does_not_block_the_next_one() {
        let mut q = vec![entry("bad", 0), entry("good", 1)];
        mark_sending(&mut q, "bad");
        on_permanent_failure(&mut q, "bad");
        assert_eq!(
            pump_decision(true, &q, 1_800_000_000_000),
            PumpDecision::Send("good".to_string())
        );
    }

    // ── The status line ──────────────────────────────────────────────────────

    #[test]
    fn the_status_counts_and_reports_the_newest_error() {
        let mut a = entry("a", 100);
        a.last_error = Some("old failure".into());
        let mut b = entry("b", 200);
        b.last_error = Some("recent failure".into());
        let mut c = entry("c", 300);
        c.status = TelemetryStatus::Failed;

        let s = queue_status(&[a, b, c]);
        assert_eq!(s.pending, 2);
        assert_eq!(s.failed, 1);
        assert_eq!(s.oldest_at, Some(100));
        assert_eq!(
            s.last_error.as_deref(),
            Some("recent failure"),
            "the newest error is the one that still describes reality"
        );
    }

    #[test]
    fn an_empty_queue_has_an_honest_status() {
        let s = queue_status(&[]);
        assert_eq!(s, TelemetryQueueStatus::default());
        assert_eq!(s.pending, 0);
        assert_eq!(s.oldest_at, None);
        assert_eq!(s.last_error, None);
    }

    #[test]
    fn the_stored_status_spelling_round_trips() {
        for s in [
            TelemetryStatus::Pending,
            TelemetryStatus::Sending,
            TelemetryStatus::Failed,
        ] {
            assert_eq!(TelemetryStatus::from_wire(s.as_str()), s);
            // The serde spelling IS the stored spelling — one word for one
            // state, so the CHECK constraint and the UI cannot disagree.
            assert_eq!(
                serde_json::to_string(&s).expect("serialises"),
                format!("\"{}\"", s.as_str())
            );
        }
        // A hand-edited value is retried, not stranded.
        assert_eq!(
            TelemetryStatus::from_wire("nonsense"),
            TelemetryStatus::Pending
        );
    }
}
