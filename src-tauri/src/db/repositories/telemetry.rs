//! E3/E5 — the telemetry repository: counters, quality rows, client state and
//! the outbox.
//!
//! Follows the house rule that commands never touch `sqlx` — but it is also the
//! implementation of [`QualityStore`], which is what
//! [`crate::telemetry::quality::QualityCollector::drain_if_quiet`] writes
//! through once its live gate opens. That indirection is the point: the
//! collector cannot reach a database except through a trait the gate hands it,
//! so there is no path from a cue dispatch to a disk write.
//!
//! **Nothing here sends anything.** E5 added the outbox and the client state
//! bag, and the *only* thing that reads the outbox is
//! [`crate::telemetry::sender`], behind the consent gate and the live gate. The
//! `sent_value` / `sent_at` watermarks E3 put in the schema are what makes a
//! drain idempotent; this module is where they are read and advanced.

use sqlx::SqlitePool;

use crate::db::{new_id, now_ms};
use crate::error::AppResult;
use crate::telemetry::counters::{CounterName, CounterTotal, ALL_COUNTERS};
use crate::telemetry::outbox::{TelemetryEntry, TelemetryStatus};
use crate::telemetry::payload::{DrainMarks, ProblemReport, ReportContext, StoredReport};
use crate::telemetry::quality::{QualityReason, QualityRow, QualityStore, QualityVerdict};

/// The CLOSED key vocabulary of `telemetry_state`.
///
/// A key/value table with free-form keys is a table anything can put anything
/// in. This enum is the whole vocabulary, and every read and write goes through
/// it, so a future caller cannot invent `telemetry.churchName` by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKey {
    /// The `ConsentRecord` JSON. **ABSENT MEANS NEVER ASKED**, which is not the
    /// same as "no" — see [`crate::telemetry::consent`].
    Consent,
    /// The install id (a bare UUID string, JSON-encoded like every other value
    /// in the bag). Absent until consent is granted.
    InstallId,
    /// Unix ms of the newest crash record already reported.
    CrashWatermark,
    /// Install ids whose remote data the operator asked to have deleted, as a
    /// JSON array of strings.
    PendingDeletions,
    /// The renderer's UI locale, mirrored so a payload can name the UI language.
    /// The locale itself lives in the renderer's `localStorage`, which Rust
    /// cannot read.
    Language,
    /// Whether an Anthropic key is in the OS keychain — a MIRROR, written by the
    /// `ai_key_*` commands. The keychain is never read from an automatic path:
    /// on macOS that can raise a blocking GUI authorisation dialog, which this
    /// codebase already refuses to risk on the go-live path (see
    /// `services::ai::keystore::resolve_noninteractive`).
    AiKeyPresent,
}

impl StateKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Consent => "telemetry.consent",
            Self::InstallId => "telemetry.installId",
            Self::CrashWatermark => "telemetry.crashWatermark",
            Self::PendingDeletions => "telemetry.pendingDeletions",
            Self::Language => "settings.language",
            Self::AiKeyPresent => "settings.aiKeyPresent",
        }
    }
}

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
        // Including the operator's own words: "delete my data" that left a
        // hand-written problem report on the machine would be a lie by omission
        // in the one place the data is unmistakably theirs.
        sqlx::query("DELETE FROM telemetry_report")
            .execute(self.pool)
            .await?;
        Ok(())
    }

    // ── E5: client state ────────────────────────────────────────────────────

    /// Read one state value. Absent is `None`, and for several keys absence is
    /// the MEANING (no consent row = never asked).
    pub async fn state_get(&self, key: StateKey) -> AppResult<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM telemetry_state WHERE key = ?1")
                .bind(key.as_str())
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(|(v,)| v))
    }

    /// Write one state value.
    pub async fn state_set(&self, key: StateKey, value: &str) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO telemetry_state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        )
        .bind(key.as_str())
        .bind(value)
        .bind(now_ms())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Remove one state value — which, for the install id, is how "this machine
    /// no longer has an identity" is expressed. Setting it to `""` would leave a
    /// row that reads as an id nobody can parse.
    pub async fn state_delete(&self, key: StateKey) -> AppResult<()> {
        sqlx::query("DELETE FROM telemetry_state WHERE key = ?1")
            .bind(key.as_str())
            .execute(self.pool)
            .await?;
        Ok(())
    }

    // ── E5: the watermark drains ────────────────────────────────────────────

    /// Every counter with a persisted row, as `(name, total, already reported)`.
    ///
    /// Only rows that EXIST: a counter that has never fired has nothing to
    /// report, and returning 19 zeroes here would make the caller filter them
    /// out again.
    pub async fn counter_watermarks(&self) -> AppResult<Vec<(CounterName, i64, i64)>> {
        let rows: Vec<(String, i64, i64)> =
            sqlx::query_as("SELECT name, value, sent_value FROM telemetry_counter ORDER BY name")
                .fetch_all(self.pool)
                .await?;
        Ok(rows
            .into_iter()
            // A name outside the closed vocabulary (a hand-edited database)
            // is ignored rather than forwarded: the allow-list is the point.
            .filter_map(|(n, v, s)| CounterName::from_wire(&n).map(|name| (name, v, s)))
            .collect())
    }

    /// Quality rows never reported, oldest first.
    pub async fn unsent_quality(&self, limit: i64) -> AppResult<Vec<QualityRow>> {
        let rows: Vec<QualityRowRecord> = sqlx::query_as(
            "SELECT id, dedupe_key, at, duration_sec, cue_count,
                    output_child_restarts, connect_timeouts, watchdog_holds,
                    dispatch_errors, companion_failures, fallback_used,
                    stale_child_reaped, abnormal_end, recovered,
                    cue_latency_p95_ms, verdict, reasons
             FROM telemetry_quality WHERE sent_at IS NULL ORDER BY at ASC LIMIT ?1",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(QualityRowRecord::into_row).collect())
    }

    /// Apply everything a delivered-or-queued payload consumed, in ONE
    /// transaction.
    ///
    /// Atomic on purpose: a process that died between "counters marked sent" and
    /// "quality marked sent" would report the quality rows a second time, and a
    /// telemetry system that double-counts is worse than one that under-counts.
    ///
    /// Counters are marked with the TOTAL the payload was built from, not with
    /// "now": an increment that landed while the payload was being written is
    /// therefore still unsent and goes in the next one.
    pub async fn commit_marks(&self, marks: &DrainMarks) -> AppResult<()> {
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        for (name, total) in &marks.counters {
            sqlx::query(
                "UPDATE telemetry_counter SET sent_value = ?2, updated_at = ?3 WHERE name = ?1",
            )
            .bind(name.as_str())
            .bind(*total)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        for id in &marks.quality_ids {
            sqlx::query("UPDATE telemetry_quality SET sent_at = ?2 WHERE id = ?1")
                .bind(id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }
        // Reports the payload ACTUALLY carries after both caps — the list the
        // builder trimmed in lockstep with the payload itself. A report the byte
        // cap deferred is not in here, so it stays owed and goes in the next one.
        for id in &marks.report_ids {
            sqlx::query("UPDATE telemetry_report SET sent_at = ?2 WHERE id = ?1")
                .bind(id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO telemetry_state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        )
        .bind(StateKey::CrashWatermark.as_str())
        .bind(marks.crash_at.to_string())
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Declare everything currently on this machine already reported.
    ///
    /// What "reporting starts today" means in code, run the moment consent is
    /// granted. A machine that has run services for two years has two years of
    /// quality rows and counter totals on disk; consent given today is consent
    /// to share what happens from today.
    pub async fn mark_everything_reported(&self, at: i64) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE telemetry_counter SET sent_value = value, updated_at = ?1")
            .bind(at)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE telemetry_quality SET sent_at = ?1 WHERE sent_at IS NULL")
            .bind(at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Drop the accumulated counters entirely. Called on a revoke: "off" must
    /// mean there is nothing left to send, not a paused pile waiting for a
    /// change of mind.
    pub async fn clear_counters(&self) -> AppResult<()> {
        sqlx::query("DELETE FROM telemetry_counter")
            .execute(self.pool)
            .await?;
        Ok(())
    }

    // ── E6: manual problem reports ──────────────────────────────────────────

    /// Store one report the operator wrote. Already scrubbed and capped by
    /// [`ProblemReport::new`] — this is a write of wire-shaped bytes.
    pub async fn insert_report(&self, stored: &StoredReport) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO telemetry_report
                 (id, at, context, message, log_tail, ephemeral, created_at, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
        )
        .bind(&stored.id)
        .bind(stored.report.at)
        .bind(stored.report.context.as_str())
        .bind(&stored.report.message)
        .bind(&stored.report.log_tail)
        .bind(i64::from(stored.ephemeral))
        .bind(now_ms())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Reports still owed a send, oldest first.
    ///
    /// `ephemeral` selects which HALF: the durable ones ride the ordinary drain
    /// under the install id, the ephemeral ones are delivered on their own under
    /// a one-shot id. The two paths must never pick up each other's rows — an
    /// ephemeral report swept into a normal payload would attach an operator who
    /// declined standing consent to this machine's permanent identity.
    pub async fn unsent_reports(
        &self,
        ephemeral: bool,
        limit: i64,
    ) -> AppResult<Vec<StoredReport>> {
        let rows: Vec<ReportRecord> = sqlx::query_as(
            "SELECT id, at, context, message, log_tail, ephemeral
             FROM telemetry_report
             WHERE sent_at IS NULL AND ephemeral = ?1
             ORDER BY at ASC LIMIT ?2",
        )
        .bind(i64::from(ephemeral))
        .bind(limit.clamp(1, 200))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(ReportRecord::into_stored).collect())
    }

    /// How many reports are still owed a send — the number behind the settings
    /// panel's "your report is waiting" line.
    pub async fn unsent_report_count(&self) -> AppResult<i64> {
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM telemetry_report WHERE sent_at IS NULL")
                .fetch_one(self.pool)
                .await?;
        Ok(n)
    }

    /// Mark reports as sent. The ONLY caller outside [`Self::commit_marks`] is
    /// the one-shot ephemeral sender, which calls it after the endpoint has
    /// accepted the bytes — never before.
    pub async fn mark_reports_sent(&self, ids: &[String], at: i64) -> AppResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for id in ids {
            sqlx::query("UPDATE telemetry_report SET sent_at = ?2 WHERE id = ?1")
                .bind(id)
                .bind(at)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Delete an ephemeral report the endpoint refused permanently.
    ///
    /// It cannot be marked "sent" — it never was — and it must not be retried
    /// forever, so the row goes and the caller says so out loud.
    pub async fn delete_report(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM telemetry_report WHERE id = ?1")
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Drop the reports that would have ridden STANDING consent, on a revoke.
    ///
    /// Returns how many. The ephemeral ones are deliberately left alone: each of
    /// those carries its own one-shot consent, given for that report and not
    /// derived from the standing answer being withdrawn here — the same argument
    /// that keeps a parked deletion alive after a revoke.
    pub async fn purge_unsent_standing_reports(&self) -> AppResult<u64> {
        let r = sqlx::query("DELETE FROM telemetry_report WHERE sent_at IS NULL AND ephemeral = 0")
            .execute(self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    // ── E5: settings reads for the payload's settings block ─────────────────

    /// How many songs the operator's libraries hold, soft-deleted rows excluded.
    /// Reported only as a BAND — see [`crate::telemetry::payload::SizeBucket`].
    pub async fn song_count(&self) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM song WHERE deleted_at IS NULL")
            .fetch_one(self.pool)
            .await?;
        Ok(n)
    }

    /// How many themes the operator made themselves. The built-ins are identical
    /// on every install, so counting them would add a constant and hide the one
    /// thing the number could say.
    pub async fn custom_theme_count(&self) -> AppResult<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM theme")
            .fetch_one(self.pool)
            .await?;
        Ok(n)
    }

    // ── E5: the outbox ──────────────────────────────────────────────────────

    /// Every queued payload, newest last.
    pub async fn outbox_load(&self) -> AppResult<Vec<TelemetryEntry>> {
        let rows: Vec<OutboxRecord> = sqlx::query_as(
            "SELECT id, created_at, schema_ver, dedup_key, payload_json, attempts,
                    next_attempt, last_error, status
             FROM telemetry_outbox ORDER BY created_at ASC",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(OutboxRecord::into_entry).collect())
    }

    /// Insert one payload and enforce the size bound, dropping the OLDEST.
    /// Returns whether a row was inserted (a duplicate `dedup_key` is not an
    /// error — it means another drain already queued this batch).
    pub async fn outbox_insert_capped(&self, entry: &TelemetryEntry) -> AppResult<bool> {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO telemetry_outbox
                 (id, created_at, schema_ver, dedup_key, payload_json, attempts,
                  next_attempt, last_error, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&entry.id)
        .bind(entry.created_at)
        .bind(entry.schema_ver as i64)
        .bind(&entry.dedup_key)
        .bind(&entry.payload_json)
        .bind(entry.attempts as i64)
        .bind(entry.next_attempt)
        .bind(entry.last_error.as_deref())
        .bind(entry.status.as_str())
        .execute(self.pool)
        .await?
        .rows_affected()
            > 0;

        if inserted {
            let victims = crate::telemetry::outbox::overflow_victims(
                &self.outbox_load().await?,
                crate::telemetry::outbox::QUEUE_MAX,
            );
            for id in victims {
                self.outbox_delete(&id).await?;
            }
        }
        Ok(inserted)
    }

    /// Persist an entry's changed lifecycle fields.
    pub async fn outbox_upsert(&self, entry: &TelemetryEntry) -> AppResult<()> {
        sqlx::query(
            "UPDATE telemetry_outbox
             SET attempts = ?2, next_attempt = ?3, last_error = ?4, status = ?5
             WHERE id = ?1",
        )
        .bind(&entry.id)
        .bind(entry.attempts as i64)
        .bind(entry.next_attempt)
        .bind(entry.last_error.as_deref())
        .bind(entry.status.as_str())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Remove one entry — a delivered payload, or one the endpoint refused
    /// permanently.
    pub async fn outbox_delete(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM telemetry_outbox WHERE id = ?1")
            .bind(id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Empty the outbox, returning how many rows went. Called on a revoke.
    pub async fn outbox_purge(&self) -> AppResult<u64> {
        Ok(sqlx::query("DELETE FROM telemetry_outbox")
            .execute(self.pool)
            .await?
            .rows_affected())
    }

    /// Return every `sending` row to `pending`, and say how many.
    ///
    /// Called ONCE at startup: an entry only reaches `sending` while a send is
    /// in flight, so at boot every one of them is stale by definition. Without
    /// this a force-quit mid-send strands the row forever.
    pub async fn outbox_reset_stale_sending(&self) -> AppResult<u64> {
        Ok(
            sqlx::query("UPDATE telemetry_outbox SET status = 'pending' WHERE status = 'sending'")
                .execute(self.pool)
                .await?
                .rows_affected(),
        )
    }
}

/// An outbox row as SQLite hands it back.
#[derive(sqlx::FromRow)]
struct OutboxRecord {
    id: String,
    created_at: i64,
    schema_ver: i64,
    dedup_key: String,
    payload_json: String,
    attempts: i64,
    next_attempt: i64,
    last_error: Option<String>,
    status: String,
}

impl OutboxRecord {
    fn into_entry(self) -> TelemetryEntry {
        TelemetryEntry {
            id: self.id,
            created_at: self.created_at,
            schema_ver: self.schema_ver.max(0) as u32,
            dedup_key: self.dedup_key,
            payload_json: self.payload_json,
            attempts: self.attempts.max(0) as u32,
            next_attempt: self.next_attempt,
            last_error: self.last_error,
            status: TelemetryStatus::from_wire(&self.status),
        }
    }
}

/// A stored report as SQLite hands it back. Kept private: the rest of the app
/// only sees [`StoredReport`].
#[derive(sqlx::FromRow)]
struct ReportRecord {
    id: String,
    at: i64,
    context: String,
    message: String,
    log_tail: String,
    ephemeral: i64,
}

impl ReportRecord {
    fn into_stored(self) -> StoredReport {
        StoredReport {
            id: self.id,
            ephemeral: self.ephemeral != 0,
            report: ProblemReport {
                at: self.at,
                // A hand-edited context reads as `other` rather than panicking —
                // and `other` is the value that claims the least.
                context: ReportContext::from_wire(&self.context).unwrap_or(ReportContext::Other),
                message: self.message,
                log_tail: self.log_tail,
            },
        }
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

    // ── E5: state, watermarks, outbox ───────────────────────────────────────

    fn outbox_entry(id: &str, created_at: i64) -> TelemetryEntry {
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

    #[tokio::test]
    async fn state_values_round_trip_and_absence_is_a_meaning() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        // A fresh install has NO consent row, and that is not "no" — it is
        // "never asked". The repository must report absence as absence.
        assert_eq!(repo.state_get(StateKey::Consent).await.expect("get"), None);

        repo.state_set(StateKey::Consent, "{\"granted\":true}")
            .await
            .expect("set");
        assert_eq!(
            repo.state_get(StateKey::Consent).await.expect("get"),
            Some("{\"granted\":true}".to_string())
        );
        // Overwriting replaces rather than duplicating.
        repo.state_set(StateKey::Consent, "{\"granted\":false}")
            .await
            .expect("set");
        assert_eq!(
            repo.state_get(StateKey::Consent).await.expect("get"),
            Some("{\"granted\":false}".to_string())
        );
        // Deleting returns it to "absent", not to an empty string.
        repo.state_delete(StateKey::Consent).await.expect("delete");
        assert_eq!(repo.state_get(StateKey::Consent).await.expect("get"), None);
        // Keys are independent.
        repo.state_set(StateKey::Language, "\"nb\"")
            .await
            .expect("set");
        assert_eq!(
            repo.state_get(StateKey::InstallId).await.expect("get"),
            None
        );
    }

    #[test]
    fn the_state_key_vocabulary_is_closed_and_unique() {
        let keys = [
            StateKey::Consent,
            StateKey::InstallId,
            StateKey::CrashWatermark,
            StateKey::PendingDeletions,
            StateKey::Language,
            StateKey::AiKeyPresent,
        ];
        let spelled: std::collections::HashSet<&str> = keys.iter().map(|k| k.as_str()).collect();
        assert_eq!(spelled.len(), keys.len(), "two keys share a spelling");
    }

    #[tokio::test]
    async fn counter_watermarks_report_deltas_and_ignore_invented_names() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 7)])
            .await
            .expect("counters");
        // A hand-edited database with a name outside the closed vocabulary.
        sqlx::query(
            "INSERT INTO telemetry_counter (name, value, sent_value, updated_at)
             VALUES ('export.Gudstjeneste 6. april', 3, 0, 0)",
        )
        .execute(&db.pool)
        .await
        .expect("tamper");

        let marks = repo.counter_watermarks().await.expect("watermarks");
        assert_eq!(
            marks,
            vec![(CounterName::LiveCueDispatched, 7, 0)],
            "the allow-list is the point: an invented name is not forwarded"
        );
    }

    #[tokio::test]
    async fn committing_marks_is_atomic_and_makes_the_data_reported() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 7)])
            .await
            .expect("counters");
        let q = row(5_000, None);
        repo.insert_quality(&q).await.expect("quality");

        assert_eq!(repo.unsent_quality(10).await.expect("unsent").len(), 1);
        repo.commit_marks(&DrainMarks {
            crash_at: 9_000,
            quality_ids: vec![q.id.clone()],
            report_ids: vec![],
            counters: vec![(CounterName::LiveCueDispatched, 7)],
        })
        .await
        .expect("commit");

        assert!(repo.unsent_quality(10).await.expect("unsent").is_empty());
        assert_eq!(
            repo.counter_watermarks().await.expect("watermarks"),
            vec![(CounterName::LiveCueDispatched, 7, 7)],
            "the counter is reported up to the total the payload carried"
        );
        assert_eq!(
            repo.state_get(StateKey::CrashWatermark).await.expect("get"),
            Some("9000".to_string())
        );
        // The row itself is NOT deleted — it is local observability the operator
        // can still read; only its "already reported" flag changed.
        assert_eq!(repo.quality_count().await.expect("count"), 1);
    }

    #[tokio::test]
    async fn an_increment_during_a_build_is_not_marked_sent() {
        // The reason marks carry the TOTAL rather than "now": a cue dispatched
        // while the payload was being written belongs to the NEXT payload, and
        // marking by timestamp would swallow it.
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 7)])
            .await
            .expect("first");
        // …payload built from a total of 7, then two more land…
        repo.add_counters(&[(CounterName::LiveCueDispatched, 2)])
            .await
            .expect("during");
        repo.commit_marks(&DrainMarks {
            crash_at: 0,
            quality_ids: vec![],
            report_ids: vec![],
            counters: vec![(CounterName::LiveCueDispatched, 7)],
        })
        .await
        .expect("commit");

        let marks = repo.counter_watermarks().await.expect("watermarks");
        assert_eq!(marks, vec![(CounterName::LiveCueDispatched, 9, 7)]);
    }

    #[tokio::test]
    async fn granting_declares_the_existing_archive_already_reported() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 400)])
            .await
            .expect("counters");
        repo.insert_quality(&row(1_000, None)).await.expect("q1");
        repo.insert_quality(&row(2_000, None)).await.expect("q2");

        repo.mark_everything_reported(9_999).await.expect("mark");

        assert!(
            repo.unsent_quality(10).await.expect("unsent").is_empty(),
            "two years of local history is not what someone said yes to today"
        );
        let marks = repo.counter_watermarks().await.expect("watermarks");
        assert_eq!(marks, vec![(CounterName::LiveCueDispatched, 400, 400)]);
    }

    #[tokio::test]
    async fn the_outbox_round_trips_and_the_bound_drops_the_oldest() {
        use crate::telemetry::outbox::QUEUE_MAX;
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);

        for i in 0..QUEUE_MAX + 3 {
            assert!(repo
                .outbox_insert_capped(&outbox_entry(&format!("id-{i:03}"), i as i64))
                .await
                .expect("insert"));
        }
        let queue = repo.outbox_load().await.expect("load");
        assert_eq!(queue.len(), QUEUE_MAX);
        assert_eq!(
            queue[0].id, "id-003",
            "the three oldest were reclaimed, not the three newest"
        );

        // A duplicate dedup_key is absorbed rather than erroring: two drains
        // racing to report the same batch produce one row.
        let dup = outbox_entry("a-different-id", 7);
        assert!(
            !repo.outbox_insert_capped(&dup).await.expect("dup"),
            "the unique dedup_key makes a second queue of one batch a no-op"
        );

        // Lifecycle fields persist.
        let mut e = repo.outbox_load().await.expect("load")[0].clone();
        e.attempts = 3;
        e.status = TelemetryStatus::Failed;
        e.last_error = Some("no route to host".into());
        e.next_attempt = 12_345;
        repo.outbox_upsert(&e).await.expect("upsert");
        let back = repo.outbox_load().await.expect("load");
        let stored = back.iter().find(|x| x.id == e.id).expect("still there");
        assert_eq!(stored.attempts, 3);
        assert_eq!(stored.status, TelemetryStatus::Failed);
        assert_eq!(stored.last_error.as_deref(), Some("no route to host"));
        assert_eq!(stored.next_attempt, 12_345);

        repo.outbox_delete(&e.id).await.expect("delete");
        assert_eq!(repo.outbox_load().await.expect("load").len(), QUEUE_MAX - 1);
        assert!(repo.outbox_purge().await.expect("purge") > 0);
        assert!(repo.outbox_load().await.expect("load").is_empty());
    }

    #[tokio::test]
    async fn a_force_quit_mid_send_is_requeued_at_startup() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        let mut e = outbox_entry("a", 1);
        repo.outbox_insert_capped(&e).await.expect("insert");
        e.status = TelemetryStatus::Sending;
        repo.outbox_upsert(&e).await.expect("upsert");

        assert_eq!(repo.outbox_reset_stale_sending().await.expect("reset"), 1);
        assert_eq!(
            repo.outbox_load().await.expect("load")[0].status,
            TelemetryStatus::Pending
        );
        // Idempotent.
        assert_eq!(repo.outbox_reset_stale_sending().await.expect("reset"), 0);
    }

    #[tokio::test]
    async fn the_settings_counts_exclude_soft_deleted_songs() {
        let db = Database::open_in_memory().await.expect("db");
        let repo = TelemetryRepo::new(&db.pool);
        assert_eq!(repo.song_count().await.expect("songs"), 0);
        assert_eq!(repo.custom_theme_count().await.expect("themes"), 0);

        let lib = crate::db::repositories::LibraryRepo::new(&db.pool)
            .create(crate::db::models::LibraryInput {
                name: "Test".into(),
                default_locale: Some("no".into()),
            })
            .await
            .expect("library");
        let song_input = |title: &str| crate::db::models::SongInput {
            library_id: lib.id.clone(),
            title: title.to_string(),
            language: None,
            default_key: None,
            tempo_bpm: None,
            ccli_song_id: None,
            tono_work_id: None,
            copyright_notice: None,
        };
        let songs = crate::db::repositories::SongRepo::new(&db.pool);
        let keep = songs
            .create(song_input("Deg være ære"))
            .await
            .expect("song");
        let gone = songs.create(song_input("Slettet")).await.expect("song");
        songs.soft_delete(&gone.id).await.expect("delete");

        assert_eq!(
            repo.song_count().await.expect("songs"),
            1,
            "a soft-deleted song is not in the library any more"
        );
        assert!(!keep.id.is_empty());
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
