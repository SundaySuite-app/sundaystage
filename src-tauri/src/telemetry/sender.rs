//! E5 — the send path, and the two gates that stand in front of it.
//!
//! ## Gate 1: the live path is hell (law 1)
//!
//! [`beat`] checks `AppState.live` FIRST, on every beat, and returns without
//! touching the database, the outbox or a socket while a service is running. A
//! `try_lock` MISS counts as live, which is the conservative reading on purpose:
//! a miss means some other thread is inside a live command at this instant.
//!
//! This does not replace E3's hand-placed drains at `live_end` / `output_close`
//! / `update_install` — those still run at the moments something interesting
//! just finished. The pump is the safety net underneath them: a machine that
//! never closes its outputs and never updates would otherwise accumulate
//! observations that are only ever written when the app exits.
//!
//! ## Gate 2: consent (law: absent means no)
//!
//! [`pump_once`] resolves consent BEFORE it loads the queue, and
//! [`super::outbox::pump_decision`] takes it as its first argument, so with
//! consent off there is no queue read, no client construction and no hostname to
//! resolve. `with_consent_off_the_sender_is_unreachable` proves it by injecting
//! a sender that PANICS if it is ever called — a flag can be asserted on and
//! forgotten, a panic fails the test wherever it happens — with a positive
//! control on the same queue so the assertion cannot pass vacuously.
//!
//! ## The exception both gates keep: deletion
//!
//! [`drain_deletions`] runs behind the LIVE gate but NOT behind the consent
//! gate, and [`should_run`] starts the pump for an owed deletion even when
//! consent is off. SundayRec shipped the opposite by accident and it made the
//! ungated deletion unreachable in exactly the case it was written for: revoke,
//! restart, press "delete my data", and the id was parked in a list nothing
//! would ever read. See [`drain_deletions`] for the full argument.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};
use tokio::task::JoinHandle;

use super::config::TelemetryEndpoint;
use super::http_sender::HttpTelemetrySender;
use super::outbox::{self, PumpDecision};
use crate::error::AppResult;
use crate::AppState;

/// Why a send did not succeed — and, crucially, whether trying again could ever
/// help.
///
/// Splitting this out is what stops the outbox doing the wrong thing with half
/// its failures. The backoff ladder is correct for a network that is down and
/// absurd for a payload the endpoint has just explained it will never accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendFailure {
    /// The network, or the endpoint, or the moment. Back off and retry.
    Transient {
        message: String,
        /// The endpoint's `Retry-After`, in ms, when it sent one.
        retry_after_ms: Option<i64>,
    },
    /// This payload will never be accepted. Drop it — after logging why.
    Permanent(String),
}

impl SendFailure {
    /// A transient failure with no `Retry-After` — the transport-error case.
    pub fn transient(message: impl Into<String>) -> Self {
        Self::Transient {
            message: message.into(),
            retry_after_ms: None,
        }
    }

    /// The message for the settings panel and the log.
    pub fn message(&self) -> &str {
        match self {
            Self::Transient { message, .. } | Self::Permanent(message) => message,
        }
    }
}

/// The future a [`TelemetrySender`] returns. Boxed by hand rather than via an
/// `async-trait` attribute so the trait stays object-safe with a plain alias.
pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), SendFailure>> + Send + 'a>>;

/// Deliver one payload, or one deletion. `Ok(())` means the endpoint accepted
/// it.
///
/// Implemented by [`HttpTelemetrySender`]. The trait lives here, with the pump,
/// so the tests below can substitute a sender that panics if it is ever called.
pub trait TelemetrySender: Send + Sync {
    fn send<'a>(&'a self, payload_json: &'a str) -> SendFuture<'a>;
    /// Ask the endpoint to erase everything for a retired install id.
    fn delete_install<'a>(&'a self, install_id: &'a str) -> SendFuture<'a>;
}

/// What one pump iteration did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpOutcome {
    /// Consent is not active. The queue was not read and the sender was not
    /// touched.
    Blocked,
    /// Nothing was due.
    Idle,
    /// One payload was delivered and removed from the outbox.
    Sent,
    /// One attempt failed transiently; the entry was backed off or retired.
    Failed,
    /// The endpoint refused the payload permanently, so it was discarded
    /// without a retry (and the reason logged).
    Dropped,
}

/// Run one iteration of the send loop.
///
/// Consent is resolved FIRST and the queue is only loaded if it is active — so
/// the blocked path performs exactly one read of one state row and returns.
pub async fn pump_once(
    pool: &SqlitePool,
    sender: &dyn TelemetrySender,
    now_ms: i64,
) -> AppResult<PumpOutcome> {
    let repo = crate::db::repositories::TelemetryRepo::new(pool);
    let active = super::client::consent_active(pool).await;
    let entries = if active {
        repo.outbox_load().await?
    } else {
        Vec::new()
    };

    let id = match outbox::pump_decision(active, &entries, now_ms) {
        PumpDecision::Blocked => return Ok(PumpOutcome::Blocked),
        PumpDecision::Idle => return Ok(PumpOutcome::Idle),
        PumpDecision::Send(id) => id,
    };

    let mut entries = entries;
    outbox::mark_sending(&mut entries, &id);
    let entry = match entries.iter().find(|e| e.id == id) {
        Some(e) => e.clone(),
        None => return Ok(PumpOutcome::Idle),
    };
    repo.outbox_upsert(&entry).await?;

    match sender.send(&entry.payload_json).await {
        Ok(()) => {
            outbox::on_success(&mut entries, &id);
            repo.outbox_delete(&id).await?;
            Ok(PumpOutcome::Sent)
        }
        Err(SendFailure::Permanent(msg)) => {
            // The endpoint will refuse these bytes every time. Retrying is not
            // patience, it is six identical rejections — so the row goes. But it
            // goes LOUDLY: a payload dropped without a word is exactly the
            // silent loss law 3 exists to prevent (the ellipsis lesson).
            //
            // Scrubbed on the way to the log, per the E3 tracing audit. Today a
            // `SendFailure` message can only be an HTTP status or one of four
            // fixed transport categories, but this line survives whatever a
            // later contributor puts in one.
            let reason = super::scrub::for_log(&msg);
            tracing::warn!(
                "telemetry: the endpoint permanently rejected a report ({reason}); it has been \
                 discarded rather than retried. This is a client/endpoint disagreement — see \
                 the endpoint's rejection log for the offending field."
            );
            outbox::on_permanent_failure(&mut entries, &id);
            repo.outbox_delete(&id).await?;
            Ok(PumpOutcome::Dropped)
        }
        Err(SendFailure::Transient {
            message,
            retry_after_ms,
        }) => {
            outbox::on_failure(&mut entries, &id, message, now_ms, retry_after_ms);
            if let Some(updated) = entries.iter().find(|x| x.id == id) {
                repo.outbox_upsert(updated).await?;
            }
            Ok(PumpOutcome::Failed)
        }
    }
}

/// Send the remote DELETE for one install id waiting for one.
///
/// The local half already ran when the operator asked:
/// [`super::client::delete_my_data`] retired the id, purged everything queued
/// under it, and parked it. Until this drains, that list only grows.
///
/// ## Why this is not behind the consent gate
///
/// Everything else in this file refuses to touch the network without active
/// consent, and this deliberately does not. A deletion request is not a
/// contribution of data — it is the withdrawal of data already contributed.
/// Gating it on consent would mean that revoking consent AND asking for deletion
/// (the obvious pair of actions) leaves the data on the server forever.
///
/// What it sends is one install id in a URL and nothing else: no payload, no
/// settings, no counters. So "with consent off, nothing about this machine is
/// transmitted" is intact — the id being sent is one the operator has explicitly
/// asked to have erased, and it is already on the server.
///
/// One id per call, oldest first, so a long list drains over successive beats
/// rather than blocking one of them. A transient failure leaves the id parked; a
/// permanent one clears it, because a 4xx here means the endpoint will never
/// accept this deletion and retrying forever is worse than stopping.
pub async fn drain_deletions(pool: &SqlitePool, sender: &dyn TelemetrySender) -> usize {
    let pending = match super::client::pending_deletions(pool).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("telemetry: could not read pending deletions: {}", e.code());
            return 0;
        }
    };
    let Some(id) = pending.first() else { return 0 };

    match sender.delete_install(id).await {
        Ok(()) => {
            tracing::info!(
                "telemetry: a retired install's data was deleted from the endpoint at the \
                 operator's request"
            );
            if let Err(e) = super::client::clear_pending_deletion(pool, id).await {
                // Not clearing means one redundant DELETE next round. The
                // endpoint treats an unknown id as a success with zero counts,
                // so that is harmless — worth saying, because the alternative
                // reading is that data comes back.
                tracing::warn!(
                    "telemetry: deletion succeeded but the id could not be cleared: {}",
                    e.code()
                );
            }
            1
        }
        Err(SendFailure::Permanent(msg)) => {
            let reason = super::scrub::for_log(&msg);
            tracing::warn!(
                "telemetry: the endpoint permanently refused a deletion request ({reason}); the \
                 id has been dropped rather than retried forever"
            );
            let _ = super::client::clear_pending_deletion(pool, id).await;
            1
        }
        Err(SendFailure::Transient { message, .. }) => {
            let reason = super::scrub::for_log(&message);
            tracing::debug!("telemetry: deletion will be retried ({reason})");
            0
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//   E6 — the one-shot report path
// ─────────────────────────────────────────────────────────────────────────────

/// Serialises the ephemeral drain against itself.
///
/// Two callers reach it: the submit command (so a report the operator just wrote
/// goes NOW rather than up to a minute later) and the beat (so one written while
/// a service was live, or while the network was down, still goes). Without this
/// they can pick up the same row and deliver it twice — and the mark is only
/// written after a success, deliberately, so there is no optimistic flag to
/// prevent it.
static REPORT_SEND_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn report_lock() -> &'static tokio::sync::Mutex<()> {
    REPORT_SEND_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Deliver ONE owed ephemeral problem report, under a one-shot identity.
///
/// Returns how many were delivered (0 or 1) — one per call, like
/// [`drain_deletions`], so a backlog drains over successive beats.
///
/// ## Why this is not behind the consent gate
///
/// The same shape of argument as deletion, from the other direction. An
/// ephemeral report exists only because the operator wrote one and pressed send
/// while standing consent was off: **that press is the consent**, given
/// explicitly, for that one report, with the exact outgoing bytes on screen
/// while they pressed. Refusing to deliver it because standing consent is off
/// would mean the app asked for permission, was given it, and then discarded it.
///
/// What it carries is one report and an envelope thin enough to act on it (app
/// version, OS, arch, UI language) — no counters, no crashes, no quality rows,
/// no settings block. And the id it travels under is generated HERE, on the spot,
/// from the same UUID source as everything else, and written nowhere: not to
/// `telemetry_state`, not to the report row, not to the outbox. Two reports sent
/// this way cannot be tied to each other, or to this machine's install id if one
/// ever exists.
///
/// The mark is written only after the endpoint has ACCEPTED the bytes. A
/// transient failure leaves the row owed; a permanent refusal deletes it, loudly,
/// because six identical rejections are not patience.
pub async fn drain_ephemeral_reports(
    pool: &SqlitePool,
    sender: &dyn TelemetrySender,
    app_version: &str,
) -> usize {
    let _guard = report_lock().lock().await;
    let repo = crate::db::repositories::TelemetryRepo::new(pool);
    let owed = match repo.unsent_reports(true, 1).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("telemetry: could not read pending reports: {}", e.code());
            return 0;
        }
    };
    let Some(stored) = owed.into_iter().next() else {
        return 0;
    };

    // The one-shot identity. Generated per SEND, not per report: a retry after a
    // failed attempt uses a different one, which is what "cannot be tied
    // together" has to mean.
    let install_id = crate::db::new_id();
    let language = super::client::language(pool).await.unwrap_or_default();
    let payload = super::payload::build_ephemeral_report_payload(
        &install_id,
        super::consent::CONSENT_VERSION,
        app_version,
        language.as_deref(),
        super::now_ms(),
        stored.report.clone(),
    );
    let json = match serde_json::to_string(&payload) {
        Ok(j) => j,
        Err(e) => {
            // The CATEGORY, never the value: a serde error quotes the input it
            // choked on, and the input here is the operator's own text. E3's
            // tracing audit exists for exactly this line.
            let category = format!("{:?}", e.classify());
            tracing::warn!(
                category,
                "telemetry: a problem report could not be serialised"
            );
            return 0;
        }
    };
    // A report is bounded at 200 + 4 000 characters, so this is unreachable
    // arithmetic rather than a real risk — but an oversized body is discarded by
    // the endpoint as permanently as a malformed one, and a report that can
    // never be delivered must not be retried forever in silence.
    if json.len() > super::payload::BODY_MAX_BYTES {
        tracing::warn!(
            bytes = json.len(),
            "telemetry: a problem report exceeded the body cap and cannot be delivered; it has \
             been dropped rather than retried forever"
        );
        let _ = repo.delete_report(&stored.id).await;
        return 0;
    }

    match sender.send(&json).await {
        Ok(()) => {
            if let Err(e) = repo.mark_reports_sent(&[stored.id], super::now_ms()).await {
                // The row stays unsent, so it is sent again with a NEW one-shot
                // id. A duplicate report is a nuisance; a lost one is the failure
                // this whole path exists to prevent.
                tracing::warn!(
                    "telemetry: a problem report was delivered but could not be marked sent \
                     ({}); it may be delivered a second time",
                    e.code()
                );
            }
            tracing::info!("telemetry: a problem report was delivered under a one-shot id");
            1
        }
        Err(SendFailure::Permanent(msg)) => {
            let reason = super::scrub::for_log(&msg);
            tracing::warn!(
                "telemetry: the endpoint permanently rejected a problem report ({reason}); it has \
                 been discarded rather than retried. The operator's words did not reach us — see \
                 the endpoint's rejection log for the offending field."
            );
            let _ = repo.delete_report(&stored.id).await;
            0
        }
        Err(SendFailure::Transient { message, .. }) => {
            let reason = super::scrub::for_log(&message);
            tracing::debug!("telemetry: a problem report will be retried ({reason})");
            0
        }
    }
}

/// Build the sender this build would use, if it has one.
///
/// Exposed so the submit command can deliver a report immediately instead of
/// waiting up to a whole [`PUMP_INTERVAL`] — the operator is looking at the
/// dialog. `None` in every build without the two build-time variables, which is
/// every development build and every build in this stage.
pub fn build_sender() -> Option<HttpTelemetrySender> {
    HttpTelemetrySender::new(TelemetryEndpoint::resolve()?)
}

// ─────────────────────────────────────────────────────────────────────────────
//   The beat
// ─────────────────────────────────────────────────────────────────────────────

/// How often the pump looks for something to do.
///
/// One minute, matching the first rung of the backoff ladder: a shorter interval
/// would spin against entries that are not due yet, and a longer one would make
/// the first rung meaningless. The loop is cheap when idle — under a closed live
/// gate it is one `try_lock` and a sleep.
pub const PUMP_INTERVAL: Duration = Duration::from_secs(60);

/// What one beat did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatOutcome {
    /// A service is live (or a live command was mid-flight). NOTHING was
    /// touched: no drain, no database write, no socket.
    pub skipped_live: bool,
    /// Quality rows folded from the in-memory buffer into SQLite.
    pub rows_flushed: usize,
    /// A payload was built and queued.
    pub queued: bool,
    pub pump: PumpOutcome,
    /// Deletion requests carried out.
    pub deletions: usize,
    /// One-shot problem reports delivered (E6).
    pub reports: usize,
}

impl Default for BeatOutcome {
    fn default() -> Self {
        Self {
            skipped_live: false,
            rows_flushed: 0,
            queued: false,
            pump: PumpOutcome::Blocked,
            deletions: 0,
            reports: 0,
        }
    }
}

/// One whole beat: gate, flush, drain, send, delete.
///
/// Split out from the loop and taking `&AppState` so the live gate is testable —
/// a unit test can construct an `AppState` but not an `AppHandle`, and "the pump
/// does nothing while a service is live" is precisely the property that must not
/// rest on an untested line.
///
/// `sender` is `None` in every build without a baked-in endpoint, which is every
/// build in this stage. The drain still runs and payloads still queue locally;
/// nothing goes anywhere.
pub async fn beat(
    state: &AppState,
    sender: Option<&dyn TelemetrySender>,
    app_version: &str,
    now_ms: i64,
) -> BeatOutcome {
    // ── Gate 1, at the very top, before anything is read ──────────────────
    if !super::quality::gate_is_open(&state.live) {
        return BeatOutcome {
            skipped_live: true,
            ..Default::default()
        };
    }

    let mut out = BeatOutcome::default();
    // E3's collector → SQLite. Carries its own live gate; it is re-checked here
    // because a service can go live between two statements.
    let flush = crate::commands::telemetry::flush(state).await;
    out.rows_flushed = flush.rows_written;
    out.skipped_live = flush.skipped_live;

    // SQLite → one queued payload. Consent-gated inside.
    match super::client::drain(&state.db.pool, &state.data_dir, app_version).await {
        Ok(queued) => out.queued = queued,
        Err(e) => tracing::warn!("telemetry: drain failed: {}", e.code()),
    }

    let Some(sender) = sender else {
        return out;
    };

    match pump_once(&state.db.pool, sender, now_ms).await {
        Ok(o) => out.pump = o,
        Err(e) => tracing::warn!("telemetry: send loop error: {}", e.code()),
    }
    // The remote half of "delete my data", on the same beat and with its own
    // consent story — see `drain_deletions`.
    out.deletions = drain_deletions(&state.db.pool, sender).await;
    // …and the operator's own words, with the third consent story: one press,
    // one report, one throwaway id. Behind the live gate at the top of this
    // function, which is what makes "reports are never sent while a service is
    // running" true rather than merely promised.
    out.reports = drain_ephemeral_reports(&state.db.pool, sender, app_version).await;
    out
}

// ─────────────────────────────────────────────────────────────────────────────
//   The supervised pump
// ─────────────────────────────────────────────────────────────────────────────

/// How long [`shutdown`] waits for the beat in flight to finish before aborting
/// it. Matches `output::process`'s grace: long enough for a beat that is mid-
/// `await` to unwind, short enough that app teardown is never noticeably slower.
const SHUTDOWN_GRACE_MS: u64 = 500;

/// Whether the pump is already running in this process.
///
/// [`maybe_spawn`] is called from three places — startup, a consent grant, and a
/// deletion request — so it has to be idempotent. Without this, an operator
/// toggling consent three times would be sending from four loops.
static PUMP_STARTED: AtomicBool = AtomicBool::new(false);

/// The running pump's handle, so app exit can take it down.
static PUMP: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();

fn pump_slot() -> &'static Mutex<Option<JoinHandle<()>>> {
    PUMP.get_or_init(|| Mutex::new(None))
}

/// Whether the pump has any work at all — the gate [`maybe_spawn`] consults
/// before it builds anything.
///
/// THREE reasons, and the two after the first are the whole point of this being
/// a function rather than an inlined `consent_active`:
///
///   1. **consent is active** — there may be reports to deliver. [`pump_once`]
///      re-checks on every beat, so a revoke mid-session stops the sending
///      without the loop having to be torn down.
///   2. **a deletion is owed** — the operator pressed "delete my data" and the
///      remote DELETE has not been carried out. Gating the LOOP on consent made
///      exactly this case unreachable in SundayRec.
///   3. **a one-shot problem report is owed** (E6) — the operator wrote one and
///      pressed send with standing consent off. The press was consent for that
///      report; a loop that would not start for it would accept the permission
///      and then throw the report away.
///
/// Everything here is the operator's own explicit request. An install that has
/// never consented and never written a report has nothing to spawn a pump for:
/// "no telemetry machinery at all on a machine that never opted in" is
/// unchanged.
pub async fn should_run(pool: &SqlitePool) -> bool {
    if super::client::consent_active(pool).await {
        return true;
    }
    if !super::client::pending_deletions(pool)
        .await
        .unwrap_or_default()
        .is_empty()
    {
        return true;
    }
    !crate::db::repositories::TelemetryRepo::new(pool)
        .unsent_reports(true, 1)
        .await
        .unwrap_or_default()
        .is_empty()
}

/// Spawn the pump, if there is anything to spawn. Returns whether a task
/// started.
///
/// Three conditions, in this order:
///
///   1. not already running;
///   2. [`should_run`] — checked BEFORE an endpoint is resolved or a client is
///      constructed, so an install with nothing to do has no task, no connection
///      pool, and nothing that could resolve a hostname;
///   3. this build has an endpoint — [`TelemetryEndpoint::resolve`] returns
///      `None` when either build variable is missing, which is every build in
///      this stage. Reports keep queueing locally, exactly as in E3.
///
/// Called at startup AND on consent AND on a deletion request. Startup alone is
/// not enough: an operator who grants consent mid-session would otherwise have
/// reports queue up until the next launch — collected, consented to, and going
/// nowhere, which looks exactly like the feature being broken.
///
/// Once started, the loop lives for the rest of the process even if consent is
/// later revoked. That is not a hole: [`pump_once`] resolves consent before it
/// reads the queue, so a running loop with consent off does one state-row read
/// per minute and touches nothing else.
pub async fn maybe_spawn(app: &AppHandle, pool: &SqlitePool) -> bool {
    if PUMP_STARTED.load(Ordering::SeqCst) {
        return false;
    }
    if !should_run(pool).await {
        tracing::debug!(
            "telemetry: consent is not active and no deletion is owed — no pump is spawned, so \
             there is nothing that could reach the network"
        );
        return false;
    }

    let Some(endpoint) = TelemetryEndpoint::resolve() else {
        tracing::info!(
            "telemetry: the pump has work to do but this build has no endpoint ({}/{} unset) — \
             reports are queued locally and nothing is sent",
            super::config::BASE_URL_VAR,
            super::config::WRITE_KEY_VAR,
        );
        return false;
    };
    let Some(sender) = HttpTelemetrySender::new(endpoint) else {
        tracing::warn!("telemetry: no HTTP client could be built — nothing is sent");
        return false;
    };

    // Claim the slot before spawning, so two concurrent calls (a startup racing
    // a consent grant) cannot both win.
    if PUMP_STARTED.swap(true, Ordering::SeqCst) {
        return false;
    }

    let app = app.clone();
    // `tokio::spawn`, matching `output::process`'s supervision tasks — the
    // handle it returns carries an `abort_handle`, which is what lets
    // `shutdown` join with a grace period and then abort the straggler.
    let handle = tokio::spawn(async move {
        let version = app.package_info().version.to_string();
        loop {
            tokio::time::sleep(PUMP_INTERVAL).await;
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let out = beat(
                state.inner(),
                Some(&sender as &dyn TelemetrySender),
                &version,
                super::now_ms(),
            )
            .await;
            if out.skipped_live {
                tracing::trace!("telemetry: a service is live — the beat did nothing");
            }
        }
    });
    *pump_slot().lock() = Some(handle);
    true
}

/// Stop the pump on app exit, following `output::process`'s teardown: give the
/// beat in flight a bounded window to finish, then abort it.
///
/// A beat holds no lock across an await that a projector could be waiting on, so
/// this is politeness rather than safety — but an aborted `sqlx` transaction is
/// a rolled-back one, and giving it half a second avoids the rollback entirely
/// in the ordinary case.
pub async fn shutdown() {
    let handle = pump_slot().lock().take();
    let Some(handle) = handle else { return };
    let abort = handle.abort_handle();
    if tokio::time::timeout(Duration::from_millis(SHUTDOWN_GRACE_MS), async {
        let _ = handle.await;
    })
    .await
    .is_err()
    {
        // The loop sleeps for a minute at a time, so the grace expiring is the
        // NORMAL path, not an anomaly — hence trace, not warn.
        tracing::trace!("telemetry: pump shutdown grace expired — aborting the sleeping beat");
        abort.abort();
    }
    PUMP_STARTED.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::TelemetryRepo;
    use crate::db::Database;
    use crate::services::cue_list::CueList;
    use crate::services::live_session::LiveSession;
    use crate::telemetry::client;
    use crate::telemetry::counters::CounterName;
    use crate::telemetry::outbox::{TelemetryEntry, TelemetryStatus, BACKOFF_STEPS_MS};
    use crate::telemetry::quality::QualityCollector;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    /// A sender that must never be called. Panicking (rather than recording a
    /// flag) is deliberate: a flag can be asserted on and forgotten, a panic
    /// fails the test wherever it happens.
    struct PanickingSender;
    impl TelemetrySender for PanickingSender {
        fn send<'a>(&'a self, _payload: &'a str) -> SendFuture<'a> {
            panic!(
                "the telemetry sender was invoked when it must not have been — this is the \
                 failure the consent and live gates exist to make impossible"
            );
        }
        fn delete_install<'a>(&'a self, _id: &'a str) -> SendFuture<'a> {
            panic!("the telemetry sender was invoked when it must not have been (deletion)");
        }
    }

    /// How a [`CountingSender`] answers.
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum Mode {
        #[default]
        Ok,
        /// The network is down — the outbox should back off.
        Transient,
        /// The endpoint says "come back in ten minutes".
        RateLimited,
        /// The endpoint refuses these bytes — the outbox should drop them.
        Permanent,
    }

    /// A sender that counts its calls, for the positive controls.
    #[derive(Clone, Default)]
    struct CountingSender {
        sends: Arc<AtomicUsize>,
        deletes: Arc<AtomicUsize>,
        /// Every payload handed to `send`, so a test can assert about the BYTES
        /// rather than about the code that produced them.
        bodies: Arc<Mutex<Vec<String>>>,
        mode: Mode,
    }

    impl CountingSender {
        fn answer(&self) -> Result<(), SendFailure> {
            match self.mode {
                Mode::Ok => Ok(()),
                Mode::Transient => Err(SendFailure::transient("no route to host")),
                Mode::RateLimited => Err(SendFailure::Transient {
                    message: "endpoint said 429 Too Many Requests".into(),
                    retry_after_ms: Some(10 * 60_000),
                }),
                Mode::Permanent => Err(SendFailure::Permanent(
                    "endpoint rejected the payload: 400 Bad Request".into(),
                )),
            }
        }
    }

    impl TelemetrySender for CountingSender {
        fn send<'a>(&'a self, payload: &'a str) -> SendFuture<'a> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            self.bodies.lock().push(payload.to_string());
            let answer = self.answer();
            Box::pin(async move { answer })
        }
        fn delete_install<'a>(&'a self, _id: &'a str) -> SendFuture<'a> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            let answer = self.answer();
            Box::pin(async move { answer })
        }
    }

    /// An `AppState` with a real in-memory database and nothing live.
    async fn app_state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::open_in_memory().await.expect("db");
        (
            AppState {
                db,
                data_dir: dir.path().to_path_buf(),
                live: Mutex::new(None),
                companion: Mutex::new(None),
                outputs: Mutex::new(None),
                pending_update: Mutex::new(None),
                telemetry: Arc::new(QualityCollector::new()),
            },
            dir,
        )
    }

    fn go_live(state: &AppState) {
        *state.live.lock() = Some(LiveSession::new(
            "svc",
            CueList {
                service_id: "svc".into(),
                compiled_at: 0,
                cues: vec![],
            },
            0,
        ));
    }

    fn entry(id: &str, created_at: i64) -> TelemetryEntry {
        TelemetryEntry {
            id: id.to_string(),
            created_at,
            schema_ver: 1,
            dedup_key: format!("quality:{created_at}"),
            payload_json: "{\"schema\":1}".to_string(),
            attempts: 0,
            next_attempt: created_at,
            last_error: None,
            status: TelemetryStatus::Pending,
        }
    }

    /// A machine with consent, something queued, and a deletion owed — i.e.
    /// everything the pump could possibly want to do.
    async fn everything_pending(state: &AppState) {
        client::consent_set(&state.db.pool, true)
            .await
            .expect("grant");
        TelemetryRepo::new(&state.db.pool)
            .outbox_insert_capped(&entry("a", 0))
            .await
            .expect("queue");
        client::regenerate_install_id(&state.db.pool)
            .await
            .expect("regenerate");
        // The regenerate purged the queue (those payloads named the old id);
        // put one back so the pump has work under the CURRENT id.
        TelemetryRepo::new(&state.db.pool)
            .outbox_insert_capped(&entry("b", 0))
            .await
            .expect("queue");
    }

    // ── Gate 1: live ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn while_a_service_is_live_the_beat_does_absolutely_nothing() {
        // THE law-1 test. A sender that panics on contact, a queue with a due
        // entry, a deletion owed, buffered quality rows — and a live session.
        let (state, _d) = app_state().await;
        everything_pending(&state).await;
        use crate::telemetry::quality::{LiveSafe, SessionOutcome};
        state.telemetry.begin_session(1_000);
        state.telemetry.finish_session(2_000, SessionOutcome::Clean);
        go_live(&state);

        let out = beat(&state, Some(&PanickingSender), "0.5.0", 1_800_000_000_000).await;

        assert!(out.skipped_live);
        assert_eq!(out.rows_flushed, 0);
        assert!(!out.queued);
        assert_eq!(out.deletions, 0);
        // Nothing reached the database either: the queued row is untouched and
        // the buffered quality row is still in memory.
        let repo = TelemetryRepo::new(&state.db.pool);
        assert_eq!(repo.outbox_load().await.expect("load").len(), 1);
        assert_eq!(repo.outbox_load().await.expect("load")[0].attempts, 0);
        assert_eq!(repo.quality_count().await.expect("count"), 0);
        assert!(!client::pending_deletions(&state.db.pool)
            .await
            .expect("parked")
            .is_empty());
    }

    #[tokio::test]
    // Holding the guard across the await is not an oversight here — it IS the
    // scenario. `beat` must see a CONTENDED `live` mutex, which is only
    // reproducible by holding it while `beat` runs. Safe: `parking_lot` cannot
    // be poisoned, the test runtime is single-threaded, and nothing inside
    // `beat` blocks on this lock (it uses `try_lock`, which is the point).
    #[allow(clippy::await_holding_lock)]
    async fn a_try_lock_miss_counts_as_live() {
        // A miss means another thread is inside a live command right now. The
        // conservative reading is the only safe one — a service could be one
        // instruction away from a cue.
        let (state, _d) = app_state().await;
        everything_pending(&state).await;
        let guard = state.live.lock();
        assert!(guard.is_none(), "no session — only the LOCK is contended");
        let out = beat(&state, Some(&PanickingSender), "0.5.0", 1_800_000_000_000).await;
        assert!(out.skipped_live, "a contended lock must read as live");
        drop(guard);
    }

    #[tokio::test]
    async fn the_positive_control_proves_the_gate_test_is_not_vacuous() {
        // The SAME state, the same beat, nothing live: the sender IS called and
        // everything happens. Without this, the test above would pass just as
        // well if the pump were never wired up at all.
        let (state, _d) = app_state().await;
        everything_pending(&state).await;
        use crate::telemetry::quality::{LiveSafe, SessionOutcome};
        state.telemetry.begin_session(1_000);
        state.telemetry.finish_session(2_000, SessionOutcome::Clean);
        let sender = CountingSender::default();

        let out = beat(&state, Some(&sender), "0.5.0", 1_800_000_000_000).await;

        assert!(!out.skipped_live);
        assert_eq!(
            out.rows_flushed, 1,
            "the buffered quality row was folded in"
        );
        assert_eq!(out.pump, PumpOutcome::Sent);
        assert_eq!(sender.sends.load(Ordering::SeqCst), 1);
        assert_eq!(out.deletions, 1);
        assert_eq!(sender.deletes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_beat_without_an_endpoint_still_drains_locally() {
        // The state every build in this stage is in: no sender at all. The local
        // half must keep working, or E3's observability quietly stops.
        let (state, _d) = app_state().await;
        client::consent_set(&state.db.pool, true)
            .await
            .expect("grant");
        TelemetryRepo::new(&state.db.pool)
            .add_counters(&[(CounterName::LiveCueDispatched, 4)])
            .await
            .expect("counters");

        let out = beat(&state, None, "0.5.0", 1_800_000_000_000).await;

        assert!(out.queued, "a payload was built and queued locally");
        assert_eq!(out.pump, PumpOutcome::Blocked, "…and nothing was sent");
        assert_eq!(
            TelemetryRepo::new(&state.db.pool)
                .outbox_load()
                .await
                .expect("load")
                .len(),
            1
        );
    }

    // ── Gate 2: consent ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn with_consent_off_the_sender_is_unreachable() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        // A queue with a due entry — everything is ready to send EXCEPT consent.
        // (Enqueued directly: the normal path would refuse to queue at all.)
        repo.outbox_insert_capped(&entry("a", 0))
            .await
            .expect("queue");

        // Never asked.
        assert_eq!(
            pump_once(pool, &PanickingSender, 1_800_000_000_000)
                .await
                .expect("pump"),
            PumpOutcome::Blocked
        );
        // Explicitly denied.
        client::consent_set(pool, false).await.expect("deny");
        // (The revoke purged the queue — put a due entry back, so the test stays
        // honest about WHY nothing was sent.)
        repo.outbox_insert_capped(&entry("b", 0))
            .await
            .expect("queue");
        assert_eq!(
            pump_once(pool, &PanickingSender, 1_800_000_000_000)
                .await
                .expect("pump"),
            PumpOutcome::Blocked
        );

        // The entry is untouched: not marked sending, not attempted.
        let back = repo.outbox_load().await.expect("load");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].status, TelemetryStatus::Pending);
        assert_eq!(back[0].attempts, 0);

        // Positive control on the same queue.
        client::consent_set(pool, true).await.expect("grant");
        repo.outbox_insert_capped(&entry("c", 0))
            .await
            .expect("queue");
        let sender = CountingSender::default();
        assert_eq!(
            pump_once(pool, &sender, 1_800_000_000_000)
                .await
                .expect("pump"),
            PumpOutcome::Sent
        );
        assert_eq!(sender.sends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_task_is_spawned_for_an_install_that_never_opted_in() {
        let (state, _d) = app_state().await;
        assert!(!should_run(&state.db.pool).await);
        client::consent_set(&state.db.pool, false)
            .await
            .expect("deny");
        assert!(!should_run(&state.db.pool).await);
        client::consent_set(&state.db.pool, true)
            .await
            .expect("grant");
        assert!(should_run(&state.db.pool).await);
    }

    // ── The deletion exception (SundayRec's lesson #92) ──────────────────────

    #[tokio::test]
    async fn a_deletion_is_owed_even_with_consent_off_and_the_pump_runs_for_it() {
        // Revoke, then ask to be forgotten. The pump must start for this alone —
        // otherwise the id sits in a list nothing will ever read, and "your data
        // is deleted" is a promise the app quietly breaks.
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        client::consent_set(pool, true).await.expect("grant");
        client::consent_set(pool, false).await.expect("revoke");
        client::delete_my_data(pool).await.expect("delete");

        assert!(!client::consent_active(pool).await);
        assert!(
            should_run(pool).await,
            "an owed deletion is work, even with consent off"
        );

        let sender = CountingSender::default();
        assert_eq!(drain_deletions(pool, &sender).await, 1);
        assert_eq!(sender.deletes.load(Ordering::SeqCst), 1);
        assert!(client::pending_deletions(pool)
            .await
            .expect("parked")
            .is_empty());
        assert!(
            !should_run(pool).await,
            "with the deletion carried out there is nothing left to run for"
        );
    }

    #[tokio::test]
    async fn the_deletion_drain_runs_on_the_beat_with_consent_off() {
        // Same lesson at the level the pump actually calls: the beat must carry
        // the deletion even though `pump_once` is Blocked.
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        client::consent_set(pool, true).await.expect("grant");
        client::consent_set(pool, false).await.expect("revoke");
        client::delete_my_data(pool).await.expect("delete");
        let sender = CountingSender::default();

        let out = beat(&state, Some(&sender), "0.5.0", 1_800_000_000_000).await;

        assert_eq!(out.pump, PumpOutcome::Blocked, "no payload may be sent");
        assert_eq!(sender.sends.load(Ordering::SeqCst), 0);
        assert_eq!(out.deletions, 1, "but the deletion IS carried out");
    }

    #[tokio::test]
    async fn deletions_drain_one_per_beat_oldest_first_and_survive_a_refusal() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        client::consent_set(pool, true).await.expect("grant");
        let first = client::install_id_if_any(pool)
            .await
            .expect("read")
            .expect("id");
        client::regenerate_install_id(pool).await.expect("again");
        let second = client::install_id_if_any(pool)
            .await
            .expect("read")
            .expect("id");
        client::regenerate_install_id(pool).await.expect("again");
        assert_eq!(
            client::pending_deletions(pool).await.expect("parked"),
            vec![first.clone(), second.clone()]
        );

        // A network failure leaves the id parked for the next beat.
        let down = CountingSender {
            mode: Mode::Transient,
            ..Default::default()
        };
        assert_eq!(drain_deletions(pool, &down).await, 0);
        assert_eq!(
            client::pending_deletions(pool).await.expect("parked").len(),
            2,
            "a deletion is never dropped because the wifi was down"
        );

        // Then the network comes back: oldest first, one per beat.
        let up = CountingSender::default();
        assert_eq!(drain_deletions(pool, &up).await, 1);
        assert_eq!(
            client::pending_deletions(pool).await.expect("parked"),
            vec![second]
        );
        assert_eq!(drain_deletions(pool, &up).await, 1);
        assert!(client::pending_deletions(pool)
            .await
            .expect("parked")
            .is_empty());
        // Nothing left to do is not an error.
        assert_eq!(drain_deletions(pool, &up).await, 0);
    }

    #[tokio::test]
    async fn a_permanently_refused_deletion_is_not_retried_forever() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        client::consent_set(pool, true).await.expect("grant");
        client::delete_my_data(pool).await.expect("delete");
        let refusing = CountingSender {
            mode: Mode::Permanent,
            ..Default::default()
        };
        assert_eq!(drain_deletions(pool, &refusing).await, 1);
        assert!(client::pending_deletions(pool)
            .await
            .expect("parked")
            .is_empty());
    }

    // ── E6: the one-shot report path ─────────────────────────────────────────

    /// Store one ephemeral report, the way the submit command does.
    async fn owe_a_report(pool: &SqlitePool, message: &str) {
        let stored = client::submit_report(
            pool,
            crate::telemetry::payload::ReportContext::Live,
            message,
            "log line",
        )
        .await
        .expect("submit");
        assert!(stored.ephemeral, "the fixture assumes consent is off");
    }

    #[tokio::test]
    async fn a_one_shot_report_goes_out_with_consent_off_and_carries_nothing_else() {
        // The owner's rule, end to end. Pressing send was consent for THIS
        // report: it is delivered, and what travels with it is an envelope and
        // nothing more.
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        // Local history that has never been consented to, so a payload that
        // swept it up would be visible here.
        repo.add_counters(&[(CounterName::LiveCueDispatched, 12)])
            .await
            .expect("counters");
        owe_a_report(pool, "projektoren ble svart").await;
        let sender = CountingSender::default();

        assert_eq!(
            drain_ephemeral_reports(pool, &sender, "0.5.0").await,
            1,
            "the report the operator explicitly sent must go"
        );

        let body = sender.bodies.lock()[0].clone();
        let v: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(v["reports"].as_array().expect("reports").len(), 1);
        assert_eq!(v["reports"][0]["message"], "projektoren ble svart");
        assert!(
            v["counters"].as_array().expect("counters").is_empty(),
            "consent is off — nothing but the report may ride along: {body}"
        );
        assert!(v["crashes"].as_array().expect("crashes").is_empty());
        assert!(v["quality"].as_array().expect("quality").is_empty());
        // The envelope keeps only what makes a bug report actionable.
        assert_eq!(v["appVersion"], "0.5.0");
        assert!(v["os"].is_string());

        assert_eq!(
            repo.unsent_report_count().await.expect("owed"),
            0,
            "delivered means marked — and only then"
        );
        assert_eq!(
            drain_ephemeral_reports(pool, &sender, "0.5.0").await,
            0,
            "…so it is not delivered twice"
        );
    }

    #[tokio::test]
    async fn the_one_shot_id_is_random_per_send_and_stored_nowhere() {
        // What "ephemeral" has to mean: no durable identity is created, the id
        // is not written to any row, and two reports cannot be tied together.
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let sender = CountingSender::default();
        owe_a_report(pool, "første").await;
        drain_ephemeral_reports(pool, &sender, "0.5.0").await;
        owe_a_report(pool, "andre").await;
        drain_ephemeral_reports(pool, &sender, "0.5.0").await;

        let bodies = sender.bodies.lock().clone();
        assert_eq!(bodies.len(), 2);
        let id_of = |b: &str| -> String {
            serde_json::from_str::<serde_json::Value>(b).expect("json")["installId"]
                .as_str()
                .expect("id")
                .to_string()
        };
        let first = id_of(&bodies[0]);
        let second = id_of(&bodies[1]);
        assert_ne!(first, second, "two one-shot sends must not share an id");
        assert_ne!(
            first,
            crate::telemetry::payload::NIL_INSTALL_ID,
            "…and must not all be the same placeholder either"
        );

        assert_eq!(
            client::install_id_if_any(pool).await.expect("read"),
            None,
            "no durable identity was created for someone who has not opted in"
        );
        // Nor is it anywhere in the state bag or the report table.
        for id in [first, second] {
            let (n,): (i64,) = sqlx::query_as(
                "SELECT (SELECT COUNT(*) FROM telemetry_state WHERE value LIKE ?1)
                       + (SELECT COUNT(*) FROM telemetry_outbox WHERE payload_json LIKE ?1)",
            )
            .bind(format!("%{id}%"))
            .fetch_one(pool)
            .await
            .expect("scan");
            assert_eq!(n, 0, "the one-shot id {id} was persisted somewhere");
        }
    }

    #[tokio::test]
    async fn a_report_is_never_sent_while_a_service_is_live() {
        // The promise the privacy text makes in as many words, and law 1 besides.
        let (state, _d) = app_state().await;
        owe_a_report(&state.db.pool, "midt i gudstjenesten").await;
        go_live(&state);

        let out = beat(&state, Some(&PanickingSender), "0.5.0", 1_800_000_000_000).await;

        assert!(out.skipped_live);
        assert_eq!(out.reports, 0);
        assert_eq!(
            TelemetryRepo::new(&state.db.pool)
                .unsent_report_count()
                .await
                .expect("owed"),
            1,
            "kept, not lost"
        );

        // …and once the service ends, the same beat delivers it.
        *state.live.lock() = None;
        let sender = CountingSender::default();
        let out = beat(&state, Some(&sender), "0.5.0", 1_800_000_000_000).await;
        assert_eq!(out.reports, 1);
    }

    #[tokio::test]
    async fn a_transient_failure_leaves_the_report_owed_and_a_refusal_drops_it() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        owe_a_report(pool, "nettverket er nede").await;

        let down = CountingSender {
            mode: Mode::Transient,
            ..Default::default()
        };
        assert_eq!(drain_ephemeral_reports(pool, &down, "0.5.0").await, 0);
        assert_eq!(
            repo.unsent_report_count().await.expect("owed"),
            1,
            "an operator's words are never dropped because the wifi was down"
        );

        // A permanent refusal is different: the endpoint will never accept these
        // bytes, so the row goes — and the log says so (the ellipsis lesson).
        let refusing = CountingSender {
            mode: Mode::Permanent,
            ..Default::default()
        };
        assert_eq!(drain_ephemeral_reports(pool, &refusing, "0.5.0").await, 0);
        assert_eq!(repo.unsent_report_count().await.expect("owed"), 0);
    }

    #[tokio::test]
    async fn the_pump_starts_for_an_owed_report_alone() {
        // Third reason in `should_run`. Without it, a report written with
        // consent off would sit in a table nothing ever reads — the app would
        // have asked for permission, been given it, and thrown the report away.
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        assert!(!should_run(pool).await);

        owe_a_report(pool, "hjelp").await;
        assert!(
            should_run(pool).await,
            "an owed one-shot report is work, even with consent off"
        );

        let sender = CountingSender::default();
        drain_ephemeral_reports(pool, &sender, "0.5.0").await;
        assert!(
            !should_run(pool).await,
            "…and with it delivered there is nothing left to run for"
        );
    }

    #[tokio::test]
    async fn a_report_written_under_consent_is_not_touched_by_the_one_shot_path() {
        // The two streams must not cross: a durable report belongs in the
        // ordinary payload, under the install id, not in a one-shot send.
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        client::consent_set(pool, true).await.expect("grant");
        client::submit_report(
            pool,
            crate::telemetry::payload::ReportContext::Editor,
            "varig",
            "log",
        )
        .await
        .expect("submit");

        let sender = CountingSender::default();
        assert_eq!(drain_ephemeral_reports(pool, &sender, "0.5.0").await, 0);
        assert_eq!(sender.sends.load(Ordering::SeqCst), 0);
        assert_eq!(
            TelemetryRepo::new(pool)
                .unsent_report_count()
                .await
                .expect("owed"),
            1,
            "still owed — by the drain, not by this path"
        );
    }

    // ── Failure handling ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_transient_failure_backs_the_entry_off_and_keeps_it() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        client::consent_set(pool, true).await.expect("grant");
        repo.outbox_insert_capped(&entry("a", 0))
            .await
            .expect("queue");
        let down = CountingSender {
            mode: Mode::Transient,
            ..Default::default()
        };

        let now = 1_800_000_000_000i64;
        assert_eq!(
            pump_once(pool, &down, now).await.expect("pump"),
            PumpOutcome::Failed
        );
        let back = repo.outbox_load().await.expect("load");
        assert_eq!(back.len(), 1, "a transient failure keeps the payload");
        assert_eq!(back[0].attempts, 1);
        assert_eq!(back[0].next_attempt, now + BACKOFF_STEPS_MS[0]);
        assert_eq!(back[0].status, TelemetryStatus::Pending);
        assert_eq!(back[0].last_error.as_deref(), Some("no route to host"));
        // …and it is not due again immediately.
        assert_eq!(
            pump_once(pool, &PanickingSender, now).await.expect("pump"),
            PumpOutcome::Idle
        );
    }

    #[tokio::test]
    async fn a_rate_limit_waits_as_long_as_the_endpoint_asked() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        client::consent_set(pool, true).await.expect("grant");
        repo.outbox_insert_capped(&entry("a", 0))
            .await
            .expect("queue");
        let limited = CountingSender {
            mode: Mode::RateLimited,
            ..Default::default()
        };

        let now = 1_800_000_000_000i64;
        assert_eq!(
            pump_once(pool, &limited, now).await.expect("pump"),
            PumpOutcome::Failed
        );
        assert_eq!(
            repo.outbox_load().await.expect("load")[0].next_attempt,
            now + 10 * 60_000,
            "the endpoint said ten minutes, so ten minutes it is"
        );
    }

    #[tokio::test]
    async fn a_permanent_rejection_drops_the_row_without_blocking_the_next() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        client::consent_set(pool, true).await.expect("grant");
        repo.outbox_insert_capped(&entry("bad", 0))
            .await
            .expect("queue");
        repo.outbox_insert_capped(&entry("good", 1))
            .await
            .expect("queue");
        let refusing = CountingSender {
            mode: Mode::Permanent,
            ..Default::default()
        };

        assert_eq!(
            pump_once(pool, &refusing, 1_800_000_000_000)
                .await
                .expect("pump"),
            PumpOutcome::Dropped
        );
        let back = repo.outbox_load().await.expect("load");
        assert_eq!(back.len(), 1, "the refused bytes are gone, not backed off");
        assert_eq!(back[0].id, "good");
        // …and the next one is sendable right away.
        let ok = CountingSender::default();
        assert_eq!(
            pump_once(pool, &ok, 1_800_000_000_000).await.expect("pump"),
            PumpOutcome::Sent
        );
    }

    #[tokio::test]
    async fn a_delivered_payload_leaves_the_outbox() {
        let (state, _d) = app_state().await;
        let pool = &state.db.pool;
        let repo = TelemetryRepo::new(pool);
        client::consent_set(pool, true).await.expect("grant");
        repo.outbox_insert_capped(&entry("a", 0))
            .await
            .expect("queue");
        let ok = CountingSender::default();
        assert_eq!(
            pump_once(pool, &ok, 1_800_000_000_000).await.expect("pump"),
            PumpOutcome::Sent
        );
        assert!(repo.outbox_load().await.expect("load").is_empty());
        assert_eq!(
            pump_once(pool, &ok, 1_800_000_000_000).await.expect("pump"),
            PumpOutcome::Idle
        );
    }

    #[test]
    fn the_beat_interval_matches_the_first_backoff_rung() {
        // A shorter interval spins against entries that are not due; a longer
        // one makes the first rung meaningless.
        assert_eq!(PUMP_INTERVAL.as_millis() as i64, BACKOFF_STEPS_MS[0]);
    }

    #[tokio::test]
    async fn shutting_down_an_unstarted_pump_is_a_no_op() {
        shutdown().await;
        shutdown().await;
    }
}
