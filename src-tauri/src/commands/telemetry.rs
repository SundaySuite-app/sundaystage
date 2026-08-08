//! E3 — local observation commands. **Nothing here sends anything anywhere.**
//!
//! Four reads and one write, all against the machine they run on:
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
//! [`log_tail`] takes NO path. The renderer names a line count and nothing
//! else, so no IPC caller can point the reader at a file of its choosing.

use tauri::State;

use crate::db::repositories::TelemetryRepo;
use crate::error::AppResult;
use crate::telemetry::counters::CounterTotal;
use crate::telemetry::logfile;
use crate::telemetry::quality::{DrainReport, QualityRow};
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
