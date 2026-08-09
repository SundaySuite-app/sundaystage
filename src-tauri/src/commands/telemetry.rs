//! E3/E5 — the `telemetry.*` command surface.
//!
//! **E3 — local observation. None of it sends anything:**
//!
//!   * [`telemetry_counters`] / [`telemetry_quality_recent`] — what has been
//!     accumulated, so the operator (and E6's privacy card) can see EXACTLY
//!     what a future release would offer to send;
//!   * [`telemetry_flush`] — fold the in-memory buffer into SQLite, if and only
//!     if no service is live;
//!   * [`telemetry_clear`] — delete the local copy;
//!   * [`log_tail`] — the last N lines of the log file, scrubbed again on the
//!     way out.
//!
//! **E5 — the client. Present, and unreachable from the shipping UI:**
//!
//!   * [`telemetry_consent_get`] / [`telemetry_consent_set`] — the three-state
//!     record. E6 builds the only UI that will ever call `set`; until then no
//!     answer is recorded, so nothing downstream runs.
//!   * [`telemetry_preview_payload`] — the real builder, read-only.
//!   * [`telemetry_queue_status`] — what is waiting in the outbox.
//!   * [`telemetry_regenerate_install_id`] / [`telemetry_delete_my_data`] — the
//!     two ways to be forgotten. Neither is consent-gated.
//!   * [`telemetry_set_language`] — mirrors the renderer's locale, which lives
//!     in `localStorage` where Rust cannot see it.
//!
//! **E6 — the consent UX's surface. The first stage where any of it runs:**
//!
//!   * [`telemetry_install_id`] — what the settings card shows next to
//!     "regenerate". Reads; never mints.
//!   * [`telemetry_report_preview`] / [`telemetry_report_submit`] — the manual
//!     problem report. The preview IS the outgoing bytes, and the submit path
//!     carries the one-shot-id rule for operators without standing consent.
//!
//! [`log_tail`] takes NO path. The renderer names a line count and nothing
//! else, so no IPC caller can point the reader at a file of its choosing.

use tauri::{AppHandle, State};

use crate::db::repositories::TelemetryRepo;
use crate::error::AppResult;
use crate::telemetry::client::{self, TelemetryPreview};
use crate::telemetry::consent::TelemetryConsent;
use crate::telemetry::counters::CounterTotal;
use crate::telemetry::logfile;
use crate::telemetry::outbox::TelemetryQueueStatus;
use crate::telemetry::payload::{ProblemReport, ReportContext};
use crate::telemetry::quality::{DrainReport, LiveSafe, QualityRow};
use crate::telemetry::sender;
use crate::AppState;

/// Default number of log lines handed back when the caller names none.
const DEFAULT_TAIL_LINES: usize = 200;

/// Every counter in the closed vocabulary: the persisted total PLUS whatever is
/// still buffered in memory, so the number the operator sees is the number that
/// happened, not the number that has reached the disk.
#[tauri::command]
pub async fn telemetry_counters(state: State<'_, AppState>) -> AppResult<Vec<CounterTotal>> {
    let mut totals = TelemetryRepo::new(&state.db.pool).counter_totals().await?;
    for (name, pending) in state.telemetry.counters().peek() {
        if let Some(t) = totals.iter_mut().find(|t| t.name == name) {
            t.value += pending as i64;
        }
    }
    Ok(totals)
}

/// The most recent persisted quality rows, newest first.
///
/// Only PERSISTED rows: a session that ended moments ago may still be in the
/// buffer waiting for the live gate, which is the honest picture of what is on
/// disk right now.
#[tauri::command]
pub async fn telemetry_quality_recent(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<QualityRow>> {
    TelemetryRepo::new(&state.db.pool)
        .recent_quality(limit.unwrap_or(20))
        .await
}

/// Fold the in-memory buffer into SQLite — if and only if no service is live.
///
/// Returns what the pass did, including whether it declined because the gate
/// was closed. Safe to call at any time from anywhere: the gate is inside.
#[tauri::command]
pub async fn telemetry_flush(state: State<'_, AppState>) -> AppResult<TelemetryFlush> {
    Ok(flush(&state).await.into())
}

/// Delete the local counters and quality rows.
#[tauri::command]
pub async fn telemetry_clear(state: State<'_, AppState>) -> AppResult<()> {
    TelemetryRepo::new(&state.db.pool).clear().await
}

/// The last `lines` lines of the log file, scrubbed on the way out.
#[tauri::command]
pub fn log_tail(lines: Option<usize>) -> AppResult<String> {
    Ok(logfile::tail(lines.unwrap_or(DEFAULT_TAIL_LINES))?)
}

// ── E5: the client ──────────────────────────────────────────────────────────

/// The consent state: what was answered, when, and the two derived questions
/// (should we ask, may we send).
#[tauri::command]
pub async fn telemetry_consent_get(state: State<'_, AppState>) -> AppResult<TelemetryConsent> {
    client::consent_get(&state.db.pool).await
}

/// Record an answer at the current scope version.
///
/// **No UI calls this yet — E6 builds the only thing that will.** Granting mints
/// the install id and starts the pump; revoking purges the outbox and the
/// accumulated counters.
#[tauri::command]
pub async fn telemetry_consent_set(
    app: AppHandle,
    state: State<'_, AppState>,
    granted: bool,
) -> AppResult<TelemetryConsent> {
    let consent = client::consent_set(&state.db.pool, granted).await?;
    // Spawn on GRANT, not only at startup: an operator who says yes mid-session
    // would otherwise have reports queue until the next launch — collected,
    // consented to, and going nowhere, which looks exactly like a broken
    // feature. The call is idempotent.
    if consent.active {
        sender::maybe_spawn(&app, &state.db.pool).await;
    }
    Ok(consent)
}

/// Become a different install: retire the current id (parking it for the remote
/// DELETE) and mint a new one.
///
/// Returns the new id, or `null` when consent is not active — an operator who is
/// not consenting must not be handed a fresh durable identifier.
#[tauri::command]
pub async fn telemetry_regenerate_install_id(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let id = client::regenerate_install_id(&state.db.pool).await?;
    // A parked deletion is work even with consent off — see
    // `sender::should_run`. This is the seam SundayRec was missing.
    sender::maybe_spawn(&app, &state.db.pool).await;
    Ok(id)
}

/// "Delete my data": retire the identity, empty the outbox, and delete the local
/// observation copy too.
///
/// Deliberately NOT consent-gated. A deletion is the withdrawal of data already
/// contributed, and it is the request an operator is most entitled to have
/// carried out — including, especially, after they have already said no.
#[tauri::command]
pub async fn telemetry_delete_my_data(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let id = client::delete_my_data(&state.db.pool).await?;
    sender::maybe_spawn(&app, &state.db.pool).await;
    Ok(id)
}

/// What is waiting in the outbox.
#[tauri::command]
pub async fn telemetry_queue_status(state: State<'_, AppState>) -> AppResult<TelemetryQueueStatus> {
    client::queue_status(&state.db.pool).await
}

/// The exact payload this machine would send, through the REAL builder.
///
/// Read-only: no install id is minted, no watermark advances, no counter is
/// spent. Calling it a hundred times changes nothing.
#[tauri::command]
pub async fn telemetry_preview_payload(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<TelemetryPreview> {
    let version = app.package_info().version.to_string();
    client::preview_payload(&state.db.pool, &state.data_dir, &version).await
}

/// The install id this machine reports under, or `null` when it has none.
///
/// Reads; never mints. A machine that has not consented has no id, and asking
/// the settings card to show one must not be the thing that creates it.
#[tauri::command]
pub async fn telemetry_install_id(state: State<'_, AppState>) -> AppResult<Option<String>> {
    client::install_id_if_any(&state.db.pool).await
}

// ── E6: the manual problem report ───────────────────────────────────────────

/// The EXACT report that pressing send would produce — the dialog's preview.
///
/// Built by the same function the submit path uses, so what is on screen is what
/// would be on the wire: scrubbed once, capped once, log tail included.
#[tauri::command]
pub fn telemetry_report_preview(context: ReportContext, message: String) -> ProblemReport {
    client::preview_report(context, &message)
}

/// What happened to a report the operator just submitted.
///
/// Every value is a true statement about bytes, not a reassurance. The dialog
/// says one of five things and never "sent" unless something was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/bindings/ReportOutcome.ts")]
#[serde(rename_all = "kebab-case")]
pub enum ReportOutcome {
    /// Standing consent is on: the report was put in a payload and queued with
    /// everything else, under the durable install id.
    Queued,
    /// Standing consent is off: the report was delivered on its own, under a
    /// one-shot id, and is gone from this machine's owed list.
    Sent,
    /// A service is live. Nothing was sent — law 1, and the promise the privacy
    /// text makes. The report is stored and goes out after the service ends.
    DeferredLive,
    /// The endpoint could not be reached. The report is stored and will be
    /// retried on a later beat.
    DeferredOffline,
    /// This build has no telemetry endpoint compiled in (every development
    /// build, and every build before the release that bakes the two variables
    /// in). Nothing can be sent from here — said plainly rather than shown as a
    /// spinner that never resolves.
    NoEndpoint,
}

/// Submit a manual problem report.
///
/// `log_tail` is the text [`telemetry_report_preview`] SHOWED, handed back so
/// the previewed bytes are the sent bytes — see [`client::submit_report`] for
/// why that round-trip is safer than re-reading the log a second later.
///
/// The two consent stories meet here:
///
///   * **standing consent on** — the report joins the ordinary drain and rides
///     the outbox under the durable install id, marked sent only when a payload
///     actually carried it;
///   * **standing consent off** — pressing send IS the consent, for this report
///     alone. It is delivered by itself under a one-shot random id that is
///     stored nowhere, and no durable identity is created.
///
/// The live gate stands in front of both: with a service running nothing is
/// sent, the report is kept, and the pump delivers it once the gate opens.
#[tauri::command]
pub async fn telemetry_report_submit(
    app: AppHandle,
    state: State<'_, AppState>,
    context: ReportContext,
    message: String,
    log_tail: String,
) -> AppResult<ReportOutcome> {
    let pool = &state.db.pool;
    let stored = client::submit_report(pool, context, &message, &log_tail).await?;

    if !stored.ephemeral {
        // Counted only for reports written under standing consent: an increment
        // for an ephemeral one would be reported LATER under the durable id, and
        // "this machine sent an anonymous report" is exactly the link the
        // one-shot id exists to break.
        state
            .telemetry
            .note_counter(crate::telemetry::counters::CounterName::ReportManualSent);
        let version = app.package_info().version.to_string();
        // Drain now rather than on the next beat: the report is the one payload
        // an operator is actively waiting on. A closed live gate is not an error
        // — `drain` is consent-gated, and the pump will pick the report up.
        if let Err(e) = client::drain(pool, &state.data_dir, &version).await {
            tracing::warn!("telemetry: drain after a report failed: {}", e.code());
        }
        sender::maybe_spawn(&app, pool).await;
        return Ok(ReportOutcome::Queued);
    }

    // The ephemeral path. Spawn the pump either way, so a report that cannot go
    // now still goes later.
    sender::maybe_spawn(&app, pool).await;
    if !crate::telemetry::quality::gate_is_open(&state.live) {
        return Ok(ReportOutcome::DeferredLive);
    }
    let Some(http) = sender::build_sender() else {
        return Ok(ReportOutcome::NoEndpoint);
    };
    let version = app.package_info().version.to_string();
    let delivered = sender::drain_ephemeral_reports(pool, &http, &version).await;
    Ok(if delivered > 0 {
        ReportOutcome::Sent
    } else {
        ReportOutcome::DeferredOffline
    })
}

/// Mirror the renderer's UI locale so a payload can name the language.
///
/// The locale lives in the renderer's `localStorage`; Rust cannot read it. The
/// value is reduced to a primary subtag on the way in, so the stored fact is
/// already the fact that would be sent.
#[tauri::command]
pub async fn telemetry_set_language(state: State<'_, AppState>, lang: String) -> AppResult<()> {
    client::set_language(&state.db.pool, &lang).await
}

/// Everything telemetry does at startup, in order.
///
/// With consent off — which is every install after this stage — it reads one
/// state row, requeues nothing, and starts a pump only if a deletion is owed.
pub async fn startup(app: &AppHandle, state: &AppState) {
    let pool = &state.db.pool;
    if !client::consent_active(pool).await {
        tracing::debug!("telemetry: consent is not active — nothing is collected or sent");
        // …with ONE exception, and it is the operator's own request rather than
        // ours: a "delete my data" parked before the app was last closed.
        // `maybe_spawn` starts nothing unless one is actually waiting.
        sender::maybe_spawn(app, pool).await;
        return;
    }
    // A force-quit mid-send strands a row in `sending` forever otherwise.
    match TelemetryRepo::new(pool).outbox_reset_stale_sending().await {
        Ok(n) if n > 0 => tracing::info!("telemetry: requeued {n} report(s) stranded mid-send"),
        Ok(_) => {}
        Err(e) => tracing::warn!("telemetry: could not reset stranded reports: {}", e.code()),
    }
    // Whatever the LAST run left behind. Nothing can be live yet at startup, so
    // this cannot contend with a service.
    let version = app.package_info().version.to_string();
    if let Err(e) = client::drain(pool, &state.data_dir, &version).await {
        tracing::warn!("telemetry: startup drain failed: {}", e.code());
    }
    sender::maybe_spawn(app, pool).await;
}

/// Drain the collector into the database. The one place the two are wired
/// together, so every caller inherits the live gate rather than remembering it.
pub async fn flush(state: &AppState) -> DrainReport {
    let repo = TelemetryRepo::new(&state.db.pool);
    state.telemetry.drain_if_quiet(&state.live, &repo).await
}

/// What a flush did, for the renderer.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryFlush.ts")]
#[serde(rename_all = "camelCase")]
pub struct TelemetryFlush {
    #[ts(type = "number")]
    pub rows_written: usize,
    #[ts(type = "number")]
    pub counters_written: usize,
    /// A service was live (or a live command was mid-flight): nothing was
    /// written, and nothing was lost.
    pub skipped_live: bool,
}

impl From<DrainReport> for TelemetryFlush {
    fn from(r: DrainReport) -> Self {
        Self {
            rows_written: r.rows_written,
            counters_written: r.counters_written,
            skipped_live: r.skipped_live,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::telemetry::counters::{CounterName, ALL_COUNTERS};
    use crate::telemetry::quality::{LiveSafe, QualityCollector, SessionOutcome};

    #[test]
    fn the_flush_report_projects_the_drain_report() {
        let report = DrainReport {
            rows_written: 2,
            counters_written: 3,
            skipped_live: true,
            write_failures: 1,
        };
        let out = TelemetryFlush::from(report);
        assert_eq!(out.rows_written, 2);
        assert_eq!(out.counters_written, 3);
        assert!(out.skipped_live);
        // `write_failures` is deliberately NOT projected: it is a local
        // diagnostic, and a renderer that saw it would only be able to worry.
        let json = serde_json::to_string(&out).expect("serialises");
        assert!(!json.contains("writeFailures"), "{json}");
        assert!(json.contains("skippedLive"), "{json}");
    }

    #[tokio::test]
    async fn counter_totals_include_what_is_still_in_memory() {
        // The read command's contract: the number shown is what HAPPENED, not
        // what has reached the disk. A service that just ended has its cues in
        // the buffer, not in SQLite.
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 10)])
            .await
            .expect("persisted");

        let collector = QualityCollector::new();
        collector.note_cue_dispatched();
        collector.note_cue_dispatched();

        let mut totals = repo.counter_totals().await.expect("totals");
        assert_eq!(totals.len(), ALL_COUNTERS.len());
        for (name, pending) in collector.counters().peek() {
            if let Some(t) = totals.iter_mut().find(|t| t.name == name) {
                t.value += pending as i64;
            }
        }
        let cues = totals
            .iter()
            .find(|t| t.name == CounterName::LiveCueDispatched)
            .expect("cue counter");
        assert_eq!(cues.value, 12, "10 on disk + 2 still buffered");
    }

    #[tokio::test]
    async fn a_flush_writes_the_buffer_and_a_second_one_writes_nothing() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        let collector = QualityCollector::new();
        let live = parking_lot::Mutex::new(None);

        collector.begin_session(1_000);
        collector.note_cue_dispatched();
        collector.finish_session(61_000, SessionOutcome::Clean);

        let first = collector.drain_if_quiet(&live, &repo).await;
        assert_eq!(first.rows_written, 1);
        assert!(!first.skipped_live);
        let second = collector.drain_if_quiet(&live, &repo).await;
        assert_eq!(second.rows_written, 0);
        assert_eq!(repo.quality_count().await.expect("count"), 1);
    }

    #[test]
    fn the_log_tail_takes_a_line_count_and_nothing_else() {
        // The signature IS the guarantee: there is no path parameter, so no
        // caller can aim the reader at `~/.ssh/id_rsa`. With no log directory
        // armed the answer is an empty string, not an error.
        let out = log_tail(Some(10)).expect("tail");
        if logfile::current_path().is_none() {
            assert_eq!(out, "");
        }
        assert!(log_tail(None).is_ok());
        assert!(
            log_tail(Some(usize::MAX)).is_ok(),
            "the count is clamped inside"
        );
    }
}
