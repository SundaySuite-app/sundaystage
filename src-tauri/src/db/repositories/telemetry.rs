//! E3 — the telemetry repository: counters and per-session quality rows.
//!
//! Follows the house rule that commands never touch `sqlx` — but it is also the
//! implementation of [`QualityStore`], which is what
//! [`crate::telemetry::quality::QualityCollector::drain_if_quiet`] writes
//! through once its live gate opens. That indirection is the point: the
//! collector cannot reach a database except through a trait the gate hands it,
//! so there is no path from a cue dispatch to a disk write.
//!
//! Nothing here sends anything. The `sent_value` / `sent_at` watermarks exist
//! in the schema so E5 can drain against them without a second migration.

use sqlx::SqlitePool;

use crate::db::{new_id, now_ms};
use crate::error::AppResult;
use crate::telemetry::counters::{CounterName, CounterTotal, ALL_COUNTERS};
use crate::telemetry::quality::{QualityReason, QualityRow, QualityStore, QualityVerdict};

pub struct TelemetryRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> TelemetryRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// Fold a batch of in-memory deltas into the persisted totals.
    ///
    /// `value = value + ?` rather than a read-modify-write, so two drains
    /// racing (they cannot today, but a future pump might) sum instead of
    /// clobbering.
    pub async fn add_counters(&self, deltas: &[(CounterName, u64)]) -> AppResult<()> {
        if deltas.is_empty() {
            return Ok(());
        }
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        for (name, delta) in deltas {
            sqlx::query(
                "INSERT INTO telemetry_counter (name, value, sent_value, updated_at)
                 VALUES (?1, ?2, 0, ?3)
                 ON CONFLICT(name) DO UPDATE
                   SET value = value + ?2, updated_at = ?3",
            )
            .bind(name.as_str())
            .bind(*delta as i64)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Every counter in the closed vocabulary with its persisted total. Names
    /// that have never fired report 0 rather than being absent, so a reader
    /// sees the whole vocabulary and not just the busy half.
    pub async fn counter_totals(&self) -> AppResult<Vec<CounterTotal>> {
        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT name, value FROM telemetry_counter")
            .fetch_all(self.pool)
            .await?;
        Ok(ALL_COUNTERS
            .into_iter()
            .map(|name| CounterTotal {
                name,
                value: rows
                    .iter()
                    .find(|(n, _)| n == name.as_str())
                    .map(|(_, v)| *v)
                    .unwrap_or(0),
            })
            .collect())
    }

    /// Persist one quality row.
    ///
    /// `INSERT OR IGNORE`: the unique partial index on `dedupe_key` turns a
    /// second launch that sees the same leftover WAL into a no-op instead of a
    /// duplicate. Returns whether a row was actually inserted.
    pub async fn insert_quality(&self, row: &QualityRow) -> AppResult<bool> {
        let reasons: Vec<&str> = row.reasons.iter().map(|r| r.as_str()).collect();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO telemetry_quality (
                 id, dedupe_key, at, duration_sec, cue_count,
                 output_child_restarts, connect_timeouts, watchdog_holds,
                 dispatch_errors, companion_failures, fallback_used,
                 stale_child_reaped, abnormal_end, recovered,
                 cue_latency_p95_ms, verdict, reasons, created_at, sent_at
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                 ?15, ?16, ?17, ?18, NULL
             )",
        )
        .bind(&row.id)
        .bind(row.dedupe_key.as_deref())
        .bind(row.at)
        .bind(row.duration_sec)
        .bind(row.cue_count)
        .bind(row.output_child_restarts)
        .bind(row.connect_timeouts)
        .bind(row.watchdog_holds)
        .bind(row.dispatch_errors)
        .bind(row.companion_failures)
        .bind(i64::from(row.fallback_used))
        .bind(i64::from(row.stale_child_reaped))
        .bind(i64::from(row.abnormal_end))
        .bind(i64::from(row.recovered))
        .bind(row.cue_latency_p95_ms.map(i64::from))
        .bind(row.verdict.as_str())
        .bind(serde_json::to_string(&reasons).unwrap_or_else(|_| "[]".into()))
        .bind(now_ms())
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// The most recent quality rows, newest first.
    pub async fn recent_quality(&self, limit: i64) -> AppResult<Vec<QualityRow>> {
        let rows: Vec<QualityRowRecord> = sqlx::query_as(
            "SELECT id, dedupe_key, at, duration_sec, cue_count,
                    output_child_restarts, connect_timeouts, watchdog_holds,
                    dispatch_errors, companion_failures, fallback_used,
                    stale_child_reaped, abnormal_end, recovered,
                    cue_latency_p95_ms, verdict, reasons
             FROM telemetry_quality ORDER BY at DESC LIMIT ?1",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(QualityRowRecord::into_row).collect())
    }

    /// How many quality rows exist. Cheaper than fetching them for a UI badge.
    pub async fn quality_count(&self) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM telemetry_quality")
            .fetch_one(self.pool)
            .await?;
        Ok(n)
    }

    /// Delete everything. Wired to the Settings card's "clear" so a "delete my
    /// data" click is honest about the local copy too.
    pub async fn clear(&self) -> AppResult<()> {
        sqlx::query("DELETE FROM telemetry_quality")
            .execute(self.pool)
            .await?;
        sqlx::query("DELETE FROM telemetry_counter")
            .execute(self.pool)
            .await?;
        Ok(())
    }
}

/// A quality row as SQLite hands it back — booleans as INTEGERs and the reason
/// list as JSON. Kept private: the rest of the app only sees [`QualityRow`].
#[derive(sqlx::FromRow)]
struct QualityRowRecord {
    id: String,
    dedupe_key: Option<String>,
    at: i64,
    duration_sec: i64,
    cue_count: i64,
    output_child_restarts: i64,
    connect_timeouts: i64,
    watchdog_holds: i64,
    dispatch_errors: i64,
    companion_failures: i64,
    fallback_used: i64,
    stale_child_reaped: i64,
    abnormal_end: i64,
    recovered: i64,
    cue_latency_p95_ms: Option<i64>,
    verdict: String,
    reasons: String,
}

impl QualityRowRecord {
    fn into_row(self) -> QualityRow {
        QualityRow {
            id: self.id,
            dedupe_key: self.dedupe_key,
            at: self.at,
            duration_sec: self.duration_sec,
            cue_count: self.cue_count,
            output_child_restarts: self.output_child_restarts,
            connect_timeouts: self.connect_timeouts,
            watchdog_holds: self.watchdog_holds,
            dispatch_errors: self.dispatch_errors,
            companion_failures: self.companion_failures,
            fallback_used: self.fallback_used != 0,
            stale_child_reaped: self.stale_child_reaped != 0,
            abnormal_end: self.abnormal_end != 0,
            recovered: self.recovered != 0,
            cue_latency_p95_ms: self.cue_latency_p95_ms.map(|v| v as u32),
            // A row whose verdict or reasons were hand-edited reads as the most
            // conservative thing it could be, never as a panic.
            verdict: QualityVerdict::from_wire(&self.verdict).unwrap_or(QualityVerdict::Fail),
            reasons: serde_json::from_str::<Vec<String>>(&self.reasons)
                .unwrap_or_default()
                .iter()
                .filter_map(|r| QualityReason::from_wire(r))
                .collect(),
        }
    }
}

/// The collector's route to the database — and the ONLY one.
#[async_trait::async_trait]
impl QualityStore for TelemetryRepo<'_> {
    async fn write_quality(&self, row: &QualityRow) -> Result<(), String> {
        // The collector cannot handle an `AppError`, and does not need to: it
        // only wants to know whether to keep the row for the next pass.
        self.insert_quality(row).await.map(|_| ()).map_err(|e| {
            // The CODE, never the message: a sqlx error can quote the failing
            // statement, and this string reaches a log file (law 2).
            e.code().to_string()
        })
    }

    async fn add_counters(&self, deltas: &[(CounterName, u64)]) -> Result<(), String> {
        TelemetryRepo::add_counters(self, deltas)
            .await
            .map_err(|e| e.code().to_string())
    }
}

/// A row id for a freshly observed session. Exposed so the collector and the
/// repository agree on the convention (UUIDv7 TEXT, per CLAUDE.md).
pub fn new_quality_id() -> String {
    new_id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn row(at: i64, dedupe: Option<&str>) -> QualityRow {
        QualityRow {
            id: new_quality_id(),
            dedupe_key: dedupe.map(str::to_string),
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

    #[tokio::test]
    async fn counters_accumulate_across_batches() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[
            (CounterName::LiveSessionStarted, 1),
            (CounterName::LiveCueDispatched, 40),
        ])
        .await
        .expect("first batch");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 2)])
            .await
            .expect("second batch");

        let totals = repo.counter_totals().await.expect("totals");
        // The WHOLE vocabulary is reported, busy or not.
        assert_eq!(totals.len(), ALL_COUNTERS.len());
        let cues = totals
            .iter()
            .find(|t| t.name == CounterName::LiveCueDispatched)
            .expect("cue counter");
        assert_eq!(cues.value, 42, "the second batch added, it did not clobber");
        let never = totals
            .iter()
            .find(|t| t.name == CounterName::ReportManualSent)
            .expect("unused counter");
        assert_eq!(never.value, 0);
        // An empty batch is a no-op, not an error.
        repo.add_counters(&[]).await.expect("empty batch");
    }

    #[tokio::test]
    async fn a_quality_row_round_trips_through_sqlite() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        let original = row(1_000, None);
        assert!(repo.insert_quality(&original).await.expect("insert"));
        let back = repo.recent_quality(10).await.expect("read");
        assert_eq!(back, vec![original]);
        assert_eq!(repo.quality_count().await.expect("count"), 1);
    }

    #[tokio::test]
    async fn a_dedupe_key_makes_reinsertion_a_no_op() {
        // The startup reconstruction sees the same leftover WAL on every launch
        // until the operator resumes or ends the session; the second launch
        // must not add a second "abnormal end" row for the same service.
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        assert!(repo
            .insert_quality(&row(1_000, Some("abnormal:1000:3")))
            .await
            .expect("first"));
        assert!(
            !repo
                .insert_quality(&row(2_000, Some("abnormal:1000:3")))
                .await
                .expect("second"),
            "the same reconstruction must not write twice"
        );
        assert_eq!(repo.quality_count().await.expect("count"), 1);
        // A different session still gets its own row.
        assert!(repo
            .insert_quality(&row(3_000, Some("abnormal:9000:1")))
            .await
            .expect("third"));
        assert_eq!(repo.quality_count().await.expect("count"), 2);
        // …and rows WITHOUT a dedupe key are never collapsed together.
        assert!(repo.insert_quality(&row(4_000, None)).await.expect("a"));
        assert!(repo.insert_quality(&row(5_000, None)).await.expect("b"));
        assert_eq!(repo.quality_count().await.expect("count"), 4);
    }

    #[tokio::test]
    async fn rows_come_back_newest_first_and_clear_empties_everything() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        for at in [3_000, 1_000, 2_000] {
            repo.insert_quality(&row(at, None)).await.expect("insert");
        }
        let ats: Vec<i64> = repo
            .recent_quality(10)
            .await
            .expect("read")
            .iter()
            .map(|r| r.at)
            .collect();
        assert_eq!(ats, vec![3_000, 2_000, 1_000]);
        // The limit is clamped, never trusted.
        assert_eq!(repo.recent_quality(0).await.expect("read").len(), 1);
        assert_eq!(repo.recent_quality(-5).await.expect("read").len(), 1);

        repo.add_counters(&[(CounterName::ThemeCreated, 3)])
            .await
            .expect("counters");
        repo.clear().await.expect("clear");
        assert_eq!(repo.quality_count().await.expect("count"), 0);
        assert!(repo
            .counter_totals()
            .await
            .expect("totals")
            .iter()
            .all(|t| t.value == 0));
    }

    #[tokio::test]
    async fn a_hand_edited_verdict_reads_as_fail_rather_than_panicking() {
        // The database file is the operator's; a value the enum does not know
        // must degrade to the most conservative reading, never to an `unwrap`.
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.insert_quality(&row(1_000, None))
            .await
            .expect("insert");
        sqlx::query("UPDATE telemetry_quality SET reasons = '[\"invented\",\"clean\"]'")
            .execute(&db.pool)
            .await
            .expect("tamper");
        let back = repo.recent_quality(1).await.expect("read");
        assert_eq!(back[0].reasons, vec![QualityReason::Clean]);
    }

    #[tokio::test]
    async fn the_store_trait_reports_a_code_not_a_message() {
        // A sqlx error can quote the failing statement, which is exactly the
        // kind of free text law 2 keeps out of a log file.
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        sqlx::query("DROP TABLE telemetry_quality")
            .execute(&db.pool)
            .await
            .expect("drop");
        let err = QualityStore::write_quality(&repo, &row(1, None))
            .await
            .expect_err("the table is gone");
        assert_eq!(err, "database");
        assert!(!err.contains("telemetry_quality"), "{err}");
    }
}
