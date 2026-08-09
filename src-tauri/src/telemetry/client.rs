//! E5 — the client's decisions: consent, identity, deletion, drain, preview.
//!
//! [`consent`](super::consent) is the pure state machine and this is its shell:
//! it reads and writes the record, mints the install id at the one moment it is
//! legitimate to have one, and turns "the operator pressed delete my data" into
//! a parked id and a purge.
//!
//! **In THIS stage nothing here ever runs in anger.** There is no consent UI
//! until E6, so no [`consent_set`] call exists in the shipping app, the record
//! stays absent, [`consent_active`] is `false`, no install id is ever minted and
//! [`drain`] returns on its first line. That is the intended end state of E5:
//! the whole pipeline is present, tested, and inert.
//!
//! ## The one thing that is NOT behind the consent gate
//!
//! [`pending_deletions`]. A deletion request is not a contribution of data — it
//! is the withdrawal of data already contributed, and the request an operator is
//! most entitled to have carried out. Gating it on consent would mean that
//! revoking consent AND asking for deletion (the obvious pair of actions) leaves
//! the data on the server forever, because the second request can no longer be
//! delivered. SundayRec learned this the hard way; see
//! [`super::sender::should_run`] for the other half of the argument.

use sqlx::SqlitePool;

use crate::db::repositories::{StateKey, TelemetryRepo};
use crate::error::AppResult;

use super::consent::{evaluate, parse_record, ConsentRecord, TelemetryConsent, CONSENT_VERSION};
use super::outbox::{TelemetryEntry, TelemetryQueueStatus, TelemetryStatus};
use super::payload::{self, GatherContext, StagePayload};

/// How many retired install ids are remembered for deletion.
///
/// An operator who presses "delete my data" ten times before the app is next
/// online has still only asked for one thing; the oldest are dropped rather than
/// growing the row without bound.
pub const MAX_PENDING_DELETIONS: usize = 10;

// ── Consent ─────────────────────────────────────────────────────────────────

/// Read the consent state. Every failure path — no row, unreadable row,
/// malformed JSON — evaluates to `NeverAsked`, i.e. not active.
pub async fn consent_get(pool: &SqlitePool) -> AppResult<TelemetryConsent> {
    let raw = TelemetryRepo::new(pool)
        .state_get(StateKey::Consent)
        .await?;
    Ok(evaluate(parse_record(raw.as_deref())))
}

/// Whether telemetry may be collected and sent right now. THE gate.
///
/// A database error reads as `false`. Failing closed is the only defensible
/// direction — a transient sqlite error must not become a send nobody agreed to.
pub async fn consent_active(pool: &SqlitePool) -> bool {
    consent_get(pool).await.map(|c| c.active).unwrap_or(false)
}

/// Record the operator's answer at the CURRENT scope version.
///
/// Granting mints the install id if there is not one yet — the first moment it
/// is legitimate to have one — and declares everything already on disk
/// **already reported**, so reporting starts today rather than back-filling an
/// archive nobody agreed to share.
///
/// Revoking is destructive on purpose: the outbox is purged and the accumulated
/// counters are dropped, so "off" means there is nothing left to send rather
/// than a paused pile waiting for a change of mind.
pub async fn consent_set(pool: &SqlitePool, granted: bool) -> AppResult<TelemetryConsent> {
    let repo = TelemetryRepo::new(pool);
    let record = ConsentRecord::decide(granted, super::now_ms());
    repo.state_set(StateKey::Consent, &serde_json::to_string(&record)?)
        .await?;

    if granted {
        let id = ensure_install_id(pool).await?;
        repo.mark_everything_reported(super::now_ms()).await?;
        set_crash_watermark(pool, super::now_ms()).await?;
        tracing::info!(
            consent_version = CONSENT_VERSION,
            install_id_present = !id.is_empty(),
            "telemetry: consent GRANTED — reporting starts from now, not from the archive"
        );
    } else {
        on_consent_revoked(pool).await?;
        tracing::info!("telemetry: consent DENIED/revoked — nothing is collected or queued");
    }
    consent_get(pool).await
}

/// Everything that must be true the instant consent stops being active.
///
/// Its own function because each later phase adds a line to it, and a revoke
/// that forgets one leaves collected data on a machine whose owner said no.
async fn on_consent_revoked(pool: &SqlitePool) -> AppResult<()> {
    let repo = TelemetryRepo::new(pool);
    let dropped = repo.outbox_purge().await?;
    repo.clear_counters().await?;
    if dropped > 0 {
        tracing::info!("telemetry: purged {dropped} queued report(s)");
    }
    Ok(())
}

// ── Install id ──────────────────────────────────────────────────────────────

/// The install id if one has been minted, WITHOUT minting one.
///
/// Every read path uses this. The preview in particular must be able to show a
/// payload before consent is granted, and it does so with
/// [`payload::NIL_INSTALL_ID`] rather than by quietly creating a real identifier
/// for someone who has not said yes.
pub async fn install_id_if_any(pool: &SqlitePool) -> AppResult<Option<String>> {
    let raw = TelemetryRepo::new(pool)
        .state_get(StateKey::InstallId)
        .await?;
    Ok(raw.and_then(|v| serde_json::from_str::<String>(&v).ok()))
}

/// The install id, minting a fresh UUID v7 on first need.
///
/// **Only called from paths that have already established that consent is
/// active.** Minting an id for someone who declined is itself a collection: it
/// is a durable, unique identifier for their machine, created because they said
/// no. There is exactly one caller, [`consent_set`], and it is on the `granted`
/// branch.
pub async fn ensure_install_id(pool: &SqlitePool) -> AppResult<String> {
    if let Some(existing) = install_id_if_any(pool).await? {
        return Ok(existing);
    }
    let id = crate::db::new_id();
    TelemetryRepo::new(pool)
        .state_set(StateKey::InstallId, &serde_json::to_string(&id)?)
        .await?;
    tracing::info!("telemetry: minted a new install id");
    Ok(id)
}

/// Become a different install: park the old id for the remote DELETE, mint a new
/// one, and purge everything queued under the old id.
///
/// Returns the NEW id — or `None` when consent is not active, because then there
/// must not be one. That is the divergence from SundayRec worth stating: Rec's
/// regenerate always mints, which is right for its flow (it is only reachable
/// from a settings card that exists under consent) but wrong as a primitive.
/// Here, an operator who has revoked and then asks to be forgotten ends up with
/// NO id at all, which is what being forgotten means.
pub async fn regenerate_install_id(pool: &SqlitePool) -> AppResult<Option<String>> {
    let repo = TelemetryRepo::new(pool);
    let previous = install_id_if_any(pool).await?;
    if let Some(old) = previous.as_deref() {
        park_for_deletion(pool, old).await?;
    }

    // Anything already queued is addressed to the retired id, and the history it
    // was built from must not be re-reported under a new one.
    repo.outbox_purge().await?;
    repo.mark_everything_reported(super::now_ms()).await?;
    set_crash_watermark(pool, super::now_ms()).await?;

    if !consent_active(pool).await {
        repo.state_delete(StateKey::InstallId).await?;
        tracing::info!(
            had_previous = previous.is_some(),
            "telemetry: install id retired; consent is not active, so no new one is minted"
        );
        return Ok(None);
    }

    let id = crate::db::new_id();
    repo.state_set(StateKey::InstallId, &serde_json::to_string(&id)?)
        .await?;
    tracing::info!(
        had_previous = previous.is_some(),
        "telemetry: install id regenerated — future reports belong to a new, unrelated install"
    );
    Ok(Some(id))
}

/// "Delete my data", the local half.
///
/// Retires the identity (parking it for the remote DELETE), empties the outbox
/// and deletes the LOCAL observation copy too — a click that says "delete my
/// data" and leaves a year of quality rows on the machine would be a lie by
/// omission.
pub async fn delete_my_data(pool: &SqlitePool) -> AppResult<Option<String>> {
    let new_id = regenerate_install_id(pool).await?;
    TelemetryRepo::new(pool).clear().await?;
    Ok(new_id)
}

/// Remember an install id whose remote data should be deleted, newest last,
/// capped at [`MAX_PENDING_DELETIONS`].
async fn park_for_deletion(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let mut ids = pending_deletions(pool).await?;
    if !ids.iter().any(|existing| existing == id) {
        ids.push(id.to_string());
    }
    if ids.len() > MAX_PENDING_DELETIONS {
        ids.drain(..ids.len() - MAX_PENDING_DELETIONS);
    }
    TelemetryRepo::new(pool)
        .state_set(StateKey::PendingDeletions, &serde_json::to_string(&ids)?)
        .await
}

/// Install ids waiting for a remote DELETE, oldest first.
///
/// A malformed row reads as empty rather than failing — a broken list must not
/// block the regenerate that is trying to append to it.
pub async fn pending_deletions(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let raw = TelemetryRepo::new(pool)
        .state_get(StateKey::PendingDeletions)
        .await?;
    Ok(raw
        .and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default())
}

/// Drop an id from the pending-deletion list, once the endpoint has confirmed.
pub async fn clear_pending_deletion(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let ids: Vec<String> = pending_deletions(pool)
        .await?
        .into_iter()
        .filter(|existing| existing != id)
        .collect();
    TelemetryRepo::new(pool)
        .state_set(StateKey::PendingDeletions, &serde_json::to_string(&ids)?)
        .await
}

// ── Mirrored facts the payload builder needs ────────────────────────────────

/// The UI locale the renderer last reported, if any.
pub async fn language(pool: &SqlitePool) -> AppResult<Option<String>> {
    let raw = TelemetryRepo::new(pool)
        .state_get(StateKey::Language)
        .await?;
    Ok(raw.and_then(|v| serde_json::from_str::<String>(&v).ok()))
}

/// Mirror the renderer's locale. Called whenever the UI language changes.
///
/// The locale lives in the renderer's `localStorage`; Rust cannot read it, and a
/// payload that could not name the UI language would be missing the one setting
/// that says which translations actually get used.
pub async fn set_language(pool: &SqlitePool, lang: &str) -> AppResult<()> {
    // Sanitised on the way IN as well as on the way out, so the stored value is
    // already a primary subtag and the bag cannot hold a free-form string.
    let clean = payload::sanitize_language(Some(lang));
    match clean {
        Some(l) => {
            TelemetryRepo::new(pool)
                .state_set(StateKey::Language, &serde_json::to_string(&l)?)
                .await
        }
        None => {
            TelemetryRepo::new(pool)
                .state_delete(StateKey::Language)
                .await
        }
    }
}

/// Whether an Anthropic key is in the OS keychain, as last mirrored.
pub async fn ai_key_mirror(pool: &SqlitePool) -> AppResult<bool> {
    let raw = TelemetryRepo::new(pool)
        .state_get(StateKey::AiKeyPresent)
        .await?;
    Ok(raw
        .and_then(|v| serde_json::from_str::<bool>(&v).ok())
        .unwrap_or(false))
}

/// Update the keychain mirror. Called by the `ai_key_*` commands, which are the
/// only paths that touch the keychain under an explicit operator action.
pub async fn set_ai_key_mirror(pool: &SqlitePool, present: bool) -> AppResult<()> {
    TelemetryRepo::new(pool)
        .state_set(StateKey::AiKeyPresent, &serde_json::to_string(&present)?)
        .await
}

/// Unix ms of the newest crash record already reported.
pub async fn crash_watermark(pool: &SqlitePool) -> AppResult<i64> {
    let raw = TelemetryRepo::new(pool)
        .state_get(StateKey::CrashWatermark)
        .await?;
    Ok(raw.and_then(|v| v.parse::<i64>().ok()).unwrap_or(0))
}

/// Set the crash watermark.
pub async fn set_crash_watermark(pool: &SqlitePool, at: i64) -> AppResult<()> {
    TelemetryRepo::new(pool)
        .state_set(StateKey::CrashWatermark, &at.to_string())
        .await
}

// ── The drain ───────────────────────────────────────────────────────────────

/// Turn whatever is newer than the watermarks into ONE queued payload.
///
/// Returns whether anything was enqueued. Does nothing at all without active
/// consent — the first statement, before any file is read.
///
/// The order matters: enqueue FIRST, then commit the marks. The reverse would
/// lose a batch if the insert failed, and re-queuing a batch is harmless (the
/// `dedup_key` unique index absorbs it) while losing one is silent.
pub async fn drain(
    pool: &SqlitePool,
    data_dir: &std::path::Path,
    app_version: &str,
) -> AppResult<bool> {
    if !consent_active(pool).await {
        return Ok(false);
    }
    let home = super::scrub::home_dir();
    let ctx = GatherContext {
        app_data_dir: data_dir,
        app_version,
        home: home.as_deref(),
        now_ms: super::now_ms(),
        consent_version: CONSENT_VERSION,
    };
    // `ensure_install_id`, not `install_id_if_any`: consent has already been
    // established active one line above, so minting is legitimate here — and the
    // alternative is worse than it looks. A payload built with no id carries
    // `NIL_INSTALL_ID`, which E4's validator rejects BY NAME (`nil_install_id`),
    // and a 400 is dropped permanently without retry. That turns a hand-cleared
    // state row into telemetry that silently never works again.
    let install_id = ensure_install_id(pool).await?;
    let (built, marks) = payload::build(
        pool,
        &ctx,
        crash_watermark(pool).await?,
        Some(install_id.as_str()),
    )
    .await?;

    if built.is_empty() {
        return Ok(false);
    }
    let queued = enqueue(pool, &built, ctx.now_ms).await?;
    TelemetryRepo::new(pool).commit_marks(&marks).await?;
    if queued {
        tracing::info!(
            crashes = built.crashes.len(),
            quality = built.quality.len(),
            counters = built.counters.len(),
            "telemetry: queued one report"
        );
    }
    Ok(queued)
}

/// Put one built payload into the outbox.
///
/// The `dedup_key` names the newest record the payload covers, so two drains
/// racing to report the same batch produce one row.
async fn enqueue(pool: &SqlitePool, p: &StagePayload, now_ms: i64) -> AppResult<bool> {
    let newest = p
        .crashes
        .iter()
        .map(|c| c.at)
        .chain(p.quality.iter().map(|q| q.at))
        .chain(p.reports.iter().map(|r| r.at))
        .max()
        .unwrap_or(now_ms);
    let kind = if !p.crashes.is_empty() {
        "crash"
    } else if !p.quality.is_empty() {
        "quality"
    } else if !p.reports.is_empty() {
        "report"
    } else {
        "counters"
    };
    let entry = TelemetryEntry {
        id: crate::db::new_id(),
        created_at: now_ms,
        schema_ver: payload::PAYLOAD_SCHEMA,
        dedup_key: format!("{kind}:{newest}"),
        payload_json: serde_json::to_string(p)?,
        attempts: 0,
        next_attempt: now_ms,
        last_error: None,
        status: TelemetryStatus::Pending,
    };
    TelemetryRepo::new(pool).outbox_insert_capped(&entry).await
}

// ── Transparency ────────────────────────────────────────────────────────────

/// What "show me what you send" hands the UI: the real payload as pretty JSON,
/// plus the two facts needed to label it honestly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/bindings/TelemetryPreview.ts")]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPreview {
    /// The payload, pretty-printed. Byte-identical in CONTENT to what would be
    /// queued — see `preview_matches_the_queued_bytes` in the tests.
    pub json: String,
    /// True when this is literally the next payload (consent is active, so the
    /// watermarks are real). False when consent is off: there is no "next", so
    /// the preview shows the whole local history instead — the honest answer to
    /// "what WOULD you send", and a better one than an empty shell.
    pub is_next_payload: bool,
    /// Whether the payload carries anything at all.
    pub is_empty: bool,
}

/// Build the preview through the REAL builder.
///
/// Read-only: no install id is minted, no watermark advances, no counter is
/// spent. Calling it a hundred times changes nothing.
pub async fn preview_payload(
    pool: &SqlitePool,
    data_dir: &std::path::Path,
    app_version: &str,
) -> AppResult<TelemetryPreview> {
    let active = consent_active(pool).await;
    // With consent on: exactly the next payload. With consent off there is no
    // "next", so show everything the machine holds.
    let since = if active {
        crash_watermark(pool).await?
    } else {
        0
    };
    let home = super::scrub::home_dir();
    let ctx = GatherContext {
        app_data_dir: data_dir,
        app_version,
        home: home.as_deref(),
        now_ms: super::now_ms(),
        consent_version: CONSENT_VERSION,
    };
    // `install_id_if_any`, never `ensure_install_id`: reading this page must not
    // mint an identifier for someone who has not opted in. They see the nil
    // UUID, which is the truth — they do not have one yet.
    let install_id = install_id_if_any(pool).await?;
    let (built, _) = payload::build(pool, &ctx, since, install_id.as_deref()).await?;
    Ok(TelemetryPreview {
        json: serde_json::to_string_pretty(&built)?,
        is_next_payload: active,
        is_empty: built.is_empty(),
    })
}

/// What is waiting in the outbox, for a settings panel that has to be honest.
pub async fn queue_status(pool: &SqlitePool) -> AppResult<TelemetryQueueStatus> {
    Ok(super::outbox::queue_status(
        &TelemetryRepo::new(pool).outbox_load().await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::telemetry::consent::ConsentStatus;
    use crate::telemetry::counters::CounterName;
    use crate::telemetry::payload::NIL_INSTALL_ID;
    use crate::telemetry::quality::{QualityReason, QualityRow, QualityVerdict};

    async fn pool() -> (SqlitePool, tempfile::TempDir) {
        let db = Database::open_in_memory().await.expect("db");
        let dir = tempfile::tempdir().expect("tempdir");
        (db.pool, dir)
    }

    fn quality_row(at: i64) -> QualityRow {
        QualityRow {
            id: crate::db::new_id(),
            dedupe_key: None,
            at,
            duration_sec: 60,
            cue_count: 3,
            output_child_restarts: 0,
            connect_timeouts: 0,
            watchdog_holds: 0,
            dispatch_errors: 0,
            companion_failures: 0,
            fallback_used: false,
            stale_child_reaped: false,
            abnormal_end: false,
            recovered: false,
            cue_latency_p95_ms: Some(12),
            verdict: QualityVerdict::Pass,
            reasons: vec![QualityReason::Clean],
        }
    }

    // ── Consent, end to end through storage ──────────────────────────────────

    #[tokio::test]
    async fn a_fresh_install_has_never_been_asked_and_has_no_id() {
        // The state this whole stage ships in: no UI asks the question, so this
        // is what every real machine looks like after E5.
        let (pool, _d) = pool().await;
        let c = consent_get(&pool).await.expect("consent");
        assert_eq!(c.status, ConsentStatus::NeverAsked);
        assert!(c.needs_prompt);
        assert!(!c.active);
        assert!(!consent_active(&pool).await);
        assert_eq!(
            install_id_if_any(&pool).await.expect("id"),
            None,
            "an id must not exist before anyone has said yes"
        );
    }

    #[tokio::test]
    async fn granting_mints_exactly_one_id_and_keeps_it() {
        let (pool, _d) = pool().await;
        let c = consent_set(&pool, true).await.expect("grant");
        assert_eq!(c.status, ConsentStatus::Granted);
        assert!(c.active);
        assert!(!c.needs_prompt);

        let id = install_id_if_any(&pool)
            .await
            .expect("read")
            .expect("minted");
        assert_eq!(
            payload::sanitize_install_id(&id),
            id,
            "the minted id must survive the wire sanitizer unchanged"
        );
        // Granting again is idempotent for the id.
        consent_set(&pool, true).await.expect("grant again");
        assert_eq!(install_id_if_any(&pool).await.expect("read"), Some(id));
    }

    #[tokio::test]
    async fn denying_records_the_answer_without_minting_an_id() {
        // Minting an id for someone who declined is itself a collection: a
        // durable unique identifier for their machine, created because they
        // said no.
        let (pool, _d) = pool().await;
        let c = consent_set(&pool, false).await.expect("deny");
        assert_eq!(c.status, ConsentStatus::Denied);
        assert!(!c.active);
        assert!(!c.needs_prompt, "a fresh 'no' must not be re-asked");
        assert_eq!(install_id_if_any(&pool).await.expect("read"), None);
    }

    #[tokio::test]
    async fn a_grant_starts_reporting_today_rather_than_from_the_archive() {
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        // Two years of local history, collected for the operator's own benefit.
        repo.add_counters(&[(CounterName::LiveCueDispatched, 4_000)])
            .await
            .expect("counters");
        repo.insert_quality(&quality_row(1_000)).await.expect("q");

        consent_set(&pool, true).await.expect("grant");

        assert!(
            !drain(&pool, dir.path(), "0.5.0").await.expect("drain"),
            "consent given today is not consent to upload the archive"
        );
        // …but what happens NEXT is reported.
        repo.add_counters(&[(CounterName::LiveCueDispatched, 1)])
            .await
            .expect("new");
        assert!(drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
        let queued = repo.outbox_load().await.expect("load");
        assert_eq!(queued.len(), 1);
        assert!(queued[0].payload_json.contains("live.cue.dispatched"));
        assert!(
            !queued[0].payload_json.contains("4001"),
            "the delta, not the lifetime total: {}",
            queued[0].payload_json
        );
    }

    #[tokio::test]
    async fn revoking_purges_the_outbox_and_the_accumulated_counters() {
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        consent_set(&pool, true).await.expect("grant");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 5)])
            .await
            .expect("counters");
        assert!(drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
        assert_eq!(repo.outbox_load().await.expect("load").len(), 1);
        repo.add_counters(&[(CounterName::ThemeCreated, 2)])
            .await
            .expect("more");

        consent_set(&pool, false).await.expect("revoke");

        assert!(
            repo.outbox_load().await.expect("load").is_empty(),
            "'off' must mean there is nothing left to send"
        );
        assert!(
            repo.counter_watermarks()
                .await
                .expect("counters")
                .is_empty(),
            "…not a paused pile waiting for a change of mind"
        );
        // The id is NOT destroyed by a revoke: the operator asked to stop
        // sending, not to be forgotten. "Delete my data" is the other button.
        assert!(install_id_if_any(&pool).await.expect("read").is_some());
    }

    #[tokio::test]
    async fn nothing_is_drained_or_queued_without_consent() {
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 9)])
            .await
            .expect("counters");
        repo.insert_quality(&quality_row(5_000)).await.expect("q");

        // Never asked.
        assert!(!drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
        // Explicitly denied.
        consent_set(&pool, false).await.expect("deny");
        assert!(!drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
        assert!(repo.outbox_load().await.expect("load").is_empty());
        // And the positive control on the SAME data, so the assertion above is
        // not passing because there was nothing to send.
        consent_set(&pool, true).await.expect("grant");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 1)])
            .await
            .expect("new");
        assert!(drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
    }

    // ── Identity + deletion ──────────────────────────────────────────────────

    #[tokio::test]
    async fn regenerating_parks_the_old_id_and_purges_what_was_queued_under_it() {
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        consent_set(&pool, true).await.expect("grant");
        let old = install_id_if_any(&pool).await.expect("read").expect("id");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 3)])
            .await
            .expect("counters");
        drain(&pool, dir.path(), "0.5.0").await.expect("drain");
        assert_eq!(repo.outbox_load().await.expect("load").len(), 1);

        let new = regenerate_install_id(&pool)
            .await
            .expect("regenerate")
            .expect("a new id under active consent");

        assert_ne!(new, old);
        assert_eq!(pending_deletions(&pool).await.expect("parked"), vec![old]);
        assert!(
            repo.outbox_load().await.expect("load").is_empty(),
            "queued payloads are addressed to the retired id"
        );
    }

    #[tokio::test]
    async fn being_forgotten_without_consent_leaves_no_id_behind() {
        // The divergence from Rec, and the reason for it: an operator who
        // revoked and then asked to be forgotten must not be handed a fresh
        // durable identifier as a parting gift.
        let (pool, _d) = pool().await;
        consent_set(&pool, true).await.expect("grant");
        let old = install_id_if_any(&pool).await.expect("read").expect("id");
        consent_set(&pool, false).await.expect("revoke");

        assert_eq!(
            delete_my_data(&pool).await.expect("delete"),
            None,
            "no new id is minted for someone who is not consenting"
        );
        assert_eq!(install_id_if_any(&pool).await.expect("read"), None);
        assert_eq!(
            pending_deletions(&pool).await.expect("parked"),
            vec![old],
            "…but the old id is still owed a remote DELETE"
        );
    }

    #[tokio::test]
    async fn delete_my_data_also_clears_the_local_copy() {
        let (pool, _d) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        consent_set(&pool, true).await.expect("grant");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 3)])
            .await
            .expect("counters");
        repo.insert_quality(&quality_row(5_000)).await.expect("q");

        delete_my_data(&pool).await.expect("delete");

        assert_eq!(repo.quality_count().await.expect("count"), 0);
        assert!(repo
            .counter_watermarks()
            .await
            .expect("counters")
            .is_empty());
    }

    #[tokio::test]
    async fn the_parking_list_is_bounded_and_never_holds_a_duplicate() {
        let (pool, _d) = pool().await;
        for i in 0..MAX_PENDING_DELETIONS + 4 {
            park_for_deletion(&pool, &format!("id-{i:03}"))
                .await
                .expect("park");
        }
        let parked = pending_deletions(&pool).await.expect("parked");
        assert_eq!(parked.len(), MAX_PENDING_DELETIONS);
        assert_eq!(parked[0], "id-004", "the oldest requests are dropped");

        // Asking twice for the same id is one request.
        park_for_deletion(&pool, "id-013").await.expect("again");
        assert_eq!(
            pending_deletions(&pool).await.expect("parked").len(),
            MAX_PENDING_DELETIONS
        );

        clear_pending_deletion(&pool, "id-013")
            .await
            .expect("clear");
        assert!(!pending_deletions(&pool)
            .await
            .expect("parked")
            .contains(&"id-013".to_string()));
    }

    #[tokio::test]
    async fn a_corrupt_parking_list_does_not_block_a_new_request() {
        let (pool, _d) = pool().await;
        TelemetryRepo::new(&pool)
            .state_set(StateKey::PendingDeletions, "{not json")
            .await
            .expect("tamper");
        assert!(pending_deletions(&pool).await.expect("parked").is_empty());
        park_for_deletion(&pool, "id-1").await.expect("park");
        assert_eq!(
            pending_deletions(&pool).await.expect("parked"),
            vec!["id-1".to_string()]
        );
    }

    // ── Mirrors ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_language_mirror_stores_only_a_primary_subtag() {
        let (pool, _d) = pool().await;
        assert_eq!(language(&pool).await.expect("lang"), None);
        set_language(&pool, "nb-NO").await.expect("set");
        assert_eq!(language(&pool).await.expect("lang"), Some("nb".into()));
        set_language(&pool, "PL").await.expect("set");
        assert_eq!(language(&pool).await.expect("lang"), Some("pl".into()));
        // Nonsense clears rather than storing a free-form string.
        set_language(&pool, "!!").await.expect("set");
        assert_eq!(language(&pool).await.expect("lang"), None);
    }

    #[tokio::test]
    async fn the_ai_key_mirror_defaults_to_absent() {
        let (pool, _d) = pool().await;
        assert!(!ai_key_mirror(&pool).await.expect("mirror"));
        set_ai_key_mirror(&pool, true).await.expect("set");
        assert!(ai_key_mirror(&pool).await.expect("mirror"));
        set_ai_key_mirror(&pool, false).await.expect("set");
        assert!(!ai_key_mirror(&pool).await.expect("mirror"));
    }

    // ── Preview ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_preview_is_byte_identical_to_what_gets_queued() {
        // The promise the whole "show me what you send" feature rests on. A
        // preview built by a second code path would be a promise about code that
        // never runs.
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        consent_set(&pool, true).await.expect("grant");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 6)])
            .await
            .expect("counters");
        repo.insert_quality(&quality_row(super::super::now_ms()))
            .await
            .expect("q");

        let preview = preview_payload(&pool, dir.path(), "0.5.0")
            .await
            .expect("preview");
        assert!(preview.is_next_payload);
        assert!(!preview.is_empty);

        assert!(drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
        let queued = &repo.outbox_load().await.expect("load")[0].payload_json;

        // `built_at` is the one field that legitimately differs (the two were
        // built moments apart), so compare everything else exactly — as VALUES,
        // which is what "the same bytes" means once pretty-printing is undone.
        let mut a: serde_json::Value = serde_json::from_str(&preview.json).expect("preview json");
        let mut b: serde_json::Value = serde_json::from_str(queued).expect("queued json");
        a["builtAt"] = serde_json::Value::Null;
        b["builtAt"] = serde_json::Value::Null;
        assert_eq!(a, b);
        // …and re-serialising both from the same structure gives literally equal
        // bytes, which is the property E6's dialog previews.
        assert_eq!(
            serde_json::to_string(&a).expect("a"),
            serde_json::to_string(&b).expect("b")
        );
    }

    #[tokio::test]
    async fn a_preview_mints_nothing_and_spends_nothing() {
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        repo.add_counters(&[(CounterName::LiveCueDispatched, 2)])
            .await
            .expect("counters");

        for _ in 0..3 {
            let p = preview_payload(&pool, dir.path(), "0.5.0")
                .await
                .expect("preview");
            assert!(
                !p.is_next_payload,
                "with consent off there is no 'next payload'"
            );
            assert!(p.json.contains(NIL_INSTALL_ID), "{}", p.json);
        }
        assert_eq!(
            install_id_if_any(&pool).await.expect("read"),
            None,
            "reading the page must not create an identity"
        );
        assert_eq!(
            repo.counter_watermarks().await.expect("counters"),
            vec![(CounterName::LiveCueDispatched, 2, 0)],
            "nothing was marked reported"
        );
    }

    #[tokio::test]
    async fn the_queue_status_reads_the_real_outbox() {
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        assert_eq!(
            queue_status(&pool).await.expect("status"),
            TelemetryQueueStatus::default()
        );
        consent_set(&pool, true).await.expect("grant");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 1)])
            .await
            .expect("counters");
        drain(&pool, dir.path(), "0.5.0").await.expect("drain");

        let s = queue_status(&pool).await.expect("status");
        assert_eq!(s.pending, 1);
        assert_eq!(s.failed, 0);
        assert!(s.oldest_at.is_some());
    }

    #[tokio::test]
    async fn a_queued_payload_never_carries_the_nil_install_id() {
        // E4's validator rejects the nil id BY NAME — every install that
        // previewed would otherwise land in one shared bucket that looks like a
        // single very busy machine. So it refuses the payload, and a refusal is
        // dropped permanently. This is the state that could produce one: consent
        // granted, but the id row gone (a hand-edited database, a restore from a
        // partial backup).
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        consent_set(&pool, true).await.expect("grant");
        repo.state_delete(StateKey::InstallId)
            .await
            .expect("lose the id");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 1)])
            .await
            .expect("counters");

        assert!(drain(&pool, dir.path(), "0.5.0").await.expect("drain"));
        let queued = &repo.outbox_load().await.expect("load")[0].payload_json;
        assert!(
            !queued.contains(NIL_INSTALL_ID),
            "a payload with the preview id would be refused forever: {queued}"
        );
        // …because the drain minted one rather than shipping a placeholder.
        let id = install_id_if_any(&pool)
            .await
            .expect("read")
            .expect("minted");
        assert!(queued.contains(&id));
    }

    #[tokio::test]
    async fn a_second_drain_with_nothing_new_queues_nothing() {
        // Idempotence at the level the pump actually calls: a beat every minute
        // must not produce a payload a minute.
        let (pool, dir) = pool().await;
        let repo = TelemetryRepo::new(&pool);
        consent_set(&pool, true).await.expect("grant");
        repo.add_counters(&[(CounterName::LiveCueDispatched, 1)])
            .await
            .expect("counters");

        assert!(drain(&pool, dir.path(), "0.5.0").await.expect("first"));
        assert!(!drain(&pool, dir.path(), "0.5.0").await.expect("second"));
        assert!(!drain(&pool, dir.path(), "0.5.0").await.expect("third"));
        assert_eq!(repo.outbox_load().await.expect("load").len(), 1);
    }
}
