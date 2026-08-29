//! Live-engine commands.
//!
//! Phase 5.1: compile a Service into a CueList.
//! Phase 5.3: drive the running [`LiveSession`] — start, dispatch operator
//! actions, snapshot, end. The session is held in `AppState` behind a mutex and
//! persisted to disk after every action for crash recovery (the Phase 5.2
//! output process independently holds the last frame if the UI dies).

use std::sync::atomic::Ordering;

use tauri::State;

use crate::db::models::SongUsageEntry;
use crate::db::now_ms;
use crate::db::repositories::{ServiceRepo, SongRepo, SongUsageRepo};
use crate::error::{AppError, AppResult};
use crate::services::companion::transport::{CompanionBroadcaster, RealtimeTransport};
use crate::services::cue_list::{CueCompiler, CueList};
use crate::services::live_session::{
    LiveAction, LiveFrame, LiveSession, LiveSessionView, OutputState,
};
use crate::services::session_store::SessionStore;
use crate::services::song_usage::{
    item_visibility, service_date, used_songs, ItemVisibility, RETENTION_DAYS,
};
use crate::services::stage_display::{builtin_stage_presets, StageDisplayConfig};
use crate::services::sundayrec_bridge::export::{chapter_markers, session_to_srt, ChapterMarker};
use crate::services::sundayrec_bridge::manifest::{build_manifest, ItemMeta, ManifestSong};
use crate::services::sundayrec_bridge::protocol::PROTOCOL_VERSION;
use crate::telemetry::quality::{LiveSafe, SessionOutcome};
use crate::AppState;

/// Built-in stage-display presets (Phase 8).
#[tauri::command]
pub fn stage_presets() -> Vec<StageDisplayConfig> {
    builtin_stage_presets()
}

/// The bridge protocol version SundayStage speaks (Phase 10.1).
#[tauri::command]
pub fn bridge_protocol_version() -> String {
    PROTOCOL_VERSION.to_string()
}

fn require_session<T>(state: &AppState, f: impl FnOnce(&LiveSession) -> T) -> AppResult<T> {
    let guard = state.live.lock();
    let session = guard
        .as_ref()
        .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
    Ok(f(session))
}

/// Chapter markers for the recording timeline, from the current session log
/// (Phase 10.2).
#[tauri::command]
pub fn bridge_chapter_markers(state: State<'_, AppState>) -> AppResult<Vec<ChapterMarker>> {
    require_session(&state, chapter_markers)
}

/// SRT captions matching the recording timeline (Phase 10.2). `ended_at`
/// defaults to now if the recording is still running.
#[tauri::command]
pub fn bridge_export_srt(state: State<'_, AppState>, ended_at: Option<i64>) -> AppResult<String> {
    let end = ended_at.unwrap_or_else(now_ms);
    require_session(&state, |s| session_to_srt(s, end))
}

/// Export the running session as a SundayRec `service-manifest.json` (Phase
/// 10.3): the setlist + chapters with the CCLI/TONO ids SundayRec reports usage
/// against. Joins the session's display timeline back to the service plan (kind
/// + song ids by `service_item_id`), which the compiled cues don't carry.
/// Returns the camelCase JSON string SundayRec's `stage_import_manifest` parses.
/// `ended_at` defaults to now if the recording is still running.
#[tauri::command]
pub async fn bridge_export_manifest(
    state: State<'_, AppState>,
    ended_at: Option<i64>,
) -> AppResult<String> {
    let end = ended_at.unwrap_or_else(now_ms);

    // Snapshot the session out of the lock so the DB join can await freely (the
    // live mutex must never be held across `.await`).
    let (session, service_id) = {
        let guard = state.live.lock();
        let s = guard
            .as_ref()
            .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
        (s.clone(), s.service_id.clone())
    };

    // Resolve planning-time metadata for every item in the service: its kind,
    // and for song items the licensing ids (the part the live session can't
    // carry). A song row that's since been deleted simply drops to "no song".
    let service_repo = ServiceRepo::new(&state.db.pool);
    let song_repo = SongRepo::new(&state.db.pool);
    let mut meta = std::collections::HashMap::new();
    for item in service_repo.items(&service_id).await? {
        let song = if item.kind == "song" {
            match &item.song_id {
                Some(song_id) => match song_repo.get(song_id).await {
                    Ok(s) => Some(ManifestSong {
                        title: Some(s.title),
                        tono_work_id: s.tono_work_id,
                        ccli_song_id: s.ccli_song_id,
                        // Stage's local catalog has no SundaySong id yet; CCLI/
                        // TONO are what the licensing report needs.
                        sundaysong_id: None,
                    }),
                    Err(_) => None,
                },
                None => None,
            }
        } else {
            None
        };
        meta.insert(
            item.id,
            ItemMeta {
                kind: item.kind,
                song,
            },
        );
    }

    let manifest = build_manifest(&session, end, &meta, None);
    serde_json::to_string(&manifest).map_err(|e| AppError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn live_compile_cue_list(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<CueList> {
    CueCompiler::new(&state.db.pool).compile(&service_id).await
}

fn store(state: &AppState) -> SessionStore {
    SessionStore::in_dir(&state.data_dir)
}

/// Push the new frame to the crash-isolated output processes, when they are
/// running (Phase 5.2). Sync + best-effort: never blocks or fails a dispatch.
/// (The legacy in-process windows get the same frame via the frontend's
/// `ss://render` event bus instead.)
fn push_to_outputs(state: &AppState, frame: &LiveFrame) {
    if let Some(supervisor) = state.outputs.lock().as_ref() {
        supervisor.render(frame.clone());
    }
}

// ── A7 — sangbruksloggen ────────────────────────────────────────────────────

/// What an ending session put on the congregation screen, computed while the
/// live lock is held. Small on purpose: the cue list stays where it is.
struct EndingSession {
    service_id: String,
    started_at: i64,
    items: Vec<ItemVisibility>,
}

/// Read the ending session's display record. Pure and O(log entries) — no DB,
/// no IO, no allocation beyond the item list — so it is safe under the lock.
///
/// **Nothing about this runs on the live path.** The dispatcher already pushes
/// the log entry this reads; the reading happens exactly where a session ENDS.
fn ending_session(state: &AppState, ended_at: i64) -> Option<EndingSession> {
    let guard = state.live.lock();
    let session = guard.as_ref()?;
    Some(EndingSession {
        service_id: session.service_id.clone(),
        started_at: session.started_at,
        items: item_visibility(session, ended_at),
    })
}

/// File the songs that provably held the congregation output, then apply the
/// retention limit.
///
/// Best-effort in every direction: a failure here loses at most one service's
/// worth of reporting data, and must never fail the command that ended the
/// service. The warning carries a seam tag and **no content** — song titles are
/// exactly what the log tail must never reach (law 2).
async fn file_song_usage(state: &AppState, ending: Option<EndingSession>) {
    let Some(ending) = ending else { return };
    if ending.items.is_empty() {
        return;
    }
    if let Err(()) = record_usage(state, &ending).await {
        tracing::warn!(seam = "song_usage", "kunne ikke skrive sangbrukslogg");
    }
}

async fn record_usage(state: &AppState, ending: &EndingSession) -> Result<(), ()> {
    let service_repo = ServiceRepo::new(&state.db.pool);
    let song_repo = SongRepo::new(&state.db.pool);
    let usage_repo = SongUsageRepo::new(&state.db.pool);

    let songs_by_item = service_repo
        .get_songs_by_item(&ending.service_id)
        .await
        .map_err(|_| ())?;
    let used = used_songs(&ending.items, &songs_by_item);
    if used.is_empty() {
        return Ok(());
    }

    // The plan's own name, as it read that day. A plan that has since been
    // deleted still gets its songs filed — the service happened.
    let service_name = service_repo
        .get(&ending.service_id)
        .await
        .map(|s| s.name)
        .unwrap_or_else(|_| "Gudstjeneste".to_string());
    // The date the service ACTUALLY ran, not the date the plan says it should
    // have: a plan reused from last Sunday still carries last Sunday's
    // `starts_at`, and the report has to be about what happened.
    let date = service_date(ending.started_at);

    for song in used {
        // Snapshot the licensing metadata now. A song deleted in February must
        // not erase the January report.
        let row = song_repo.get(&song.song_id).await.ok();
        let author = song_repo
            .author_names(&song.song_id)
            .await
            .unwrap_or_default();
        let entry = SongUsageEntry {
            service_id: ending.service_id.clone(),
            service_name: service_name.clone(),
            service_date: date.clone(),
            song_id: song.song_id.clone(),
            // The catalog title wins; the cue's title is the fallback for a
            // song row that vanished between the service and this write.
            title: row.as_ref().map(|s| s.title.clone()).unwrap_or(song.title),
            author: (!author.is_empty()).then(|| author.join(", ")),
            ccli_song_id: row.as_ref().and_then(|s| s.ccli_song_id.clone()),
            tono_work_id: row.as_ref().and_then(|s| s.tono_work_id.clone()),
            copyright_notice: row.as_ref().and_then(|s| s.copyright_notice.clone()),
            first_shown_at: song.first_at,
            last_shown_at: song.last_at,
            visible_ms: song.visible_ms,
            show_count: song.show_count,
        };
        usage_repo.record(&entry).await.map_err(|_| ())?;
    }

    // The retention limit, applied where new rows appear. Two years back from
    // the service that just ended.
    let cutoff = ending.started_at - RETENTION_DAYS * 24 * 60 * 60 * 1_000;
    let _ = usage_repo.prune_before(cutoff).await;
    Ok(())
}

/// Compile the service and start a live session (replacing any previous one).
#[tauri::command]
pub async fn live_start(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<LiveSessionView> {
    start_session(&state, service_id).await
}

/// The body of [`live_start`], over a plain `&AppState` so the session
/// bookkeeping is provable without a Tauri runtime.
async fn start_session(state: &AppState, service_id: String) -> AppResult<LiveSessionView> {
    // Compile first (async, no lock held), then install the session. The
    // compile may be slow on a big service, but it can NOT false-time-out the
    // output watchdog: the supervisor's heartbeat pump runs on its own tokio
    // task (see `output::process`), so awaiting the compile here yields to the
    // runtime and the beats keep flowing — and even on a missed beat the child
    // *holds the last frame* rather than blanking (see `output::Watchdog`).
    let cue_list = CueCompiler::new(&state.db.pool)
        .compile(&service_id)
        .await?;
    let session = LiveSession::new(service_id, cue_list, now_ms());
    let view = session.view();
    // Best-effort WAL; a failed write must never block going live.
    let _ = store(state).begin(&session);
    // Phase 12.2 — stand up the companion broadcaster for this service. The
    // transport is a no-op until the cloud layer is configured, so this never
    // affects the live output. Continue the `seq` stream from any broadcaster
    // still installed for a re-used service_id: a phone subscribed to
    // `companion:<svc>` drops frames whose `seq <= lastSeq`, so a restart that
    // re-zeroed `seq` would freeze every already-connected phone.
    let start_seq = {
        let mut guard = state.companion.lock();
        let start_seq = guard.as_ref().map(|b| b.next_seq()).unwrap_or(0);
        *guard = Some(CompanionBroadcaster::resuming(
            &view.service_id,
            RealtimeTransport::local_only(),
            start_seq,
        ));
        start_seq
    };
    // Persist the seed seq so an immediate crash recovers the true stream
    // position — `begin` just truncated the WAL, so its length is 0 here.
    let _ = store(state).record_seq(start_seq);
    // E3 — a session this one REPLACES gets its row before the counters are
    // reset, or the morning rehearsal's cues, restarts and failures would be
    // filed against the service that follows it (and the rehearsal would leave
    // no trace at all). `Clean`, not `Abnormal`: the operator moved on by
    // deliberately starting another service — nothing crashed, and
    // `abnormal-end` is the code for a session the PROCESS never finished.
    if state.live.lock().is_some() {
        let ended_at = now_ms();
        // A7 — the rehearsal sang too. Its usage is filed against the same
        // (service, date), so it merges with the 11:00 service rather than
        // double-counting.
        let ending = ending_session(state, ended_at);
        state
            .telemetry
            .finish_session(ended_at, SessionOutcome::Clean);
        file_song_usage(state, ending).await;
    }
    // …then one atomic store and one counter increment. This is the whole cost
    // of observing a service, and it happens before the session is installed so
    // nothing can be missed between `live.lock()` and the first cue.
    state.telemetry.begin_session(view.started_at);
    *state.live.lock() = Some(session);
    // A crash-recovery offer, if one was standing, is answered: this IS the
    // operator's answer.
    state.recovery_offer.store(false, Ordering::SeqCst);
    push_to_outputs(state, &view.frame);
    Ok(view)
}

/// The Supabase Realtime channel the companion PWA must join for the running
/// service, or `None` if no service is live (Phase 12.2).
#[tauri::command]
pub fn companion_channel(state: State<'_, AppState>) -> AppResult<Option<String>> {
    Ok(state
        .companion
        .lock()
        .as_ref()
        .map(|b| b.channel().to_string()))
}

/// Re-broadcast the current frame to the companion channel (Phase 12.2). Used
/// when a phone joins mid-service and needs the current slide, or to manually
/// re-push. Returns the assigned `seq`, or an error if no service is live.
#[tauri::command]
pub fn companion_broadcast(state: State<'_, AppState>) -> AppResult<u32> {
    let frame = {
        let guard = state.live.lock();
        let session = guard
            .as_ref()
            .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
        session.current_frame()
    };
    let mut guard = state.companion.lock();
    let broadcaster = guard
        .as_mut()
        .ok_or_else(|| AppError::Validation("ingen aktiv companion-kringkasting".into()))?;
    let seq = broadcaster
        .on_cue_advance(&frame, false)
        .map_err(AppError::Internal)?;
    // This re-push advanced `seq` without an action-log entry, so persist the
    // new position; otherwise crash recovery would resume below it and re-freeze
    // the very phone whose join triggered this broadcast.
    let next = broadcaster.next_seq();
    drop(guard);
    let _ = store(&state).record_seq(next);
    Ok(seq)
}

/// Recompile the running service and swap the fresh cue list into the live
/// session — the operator added a verse or edited the plan mid-service. The
/// cue on air stays on air (remapped by cue id; see
/// [`LiveSession::replace_cue_list`]) and the output override is preserved.
#[tauri::command]
pub async fn live_reload_cue_list(state: State<'_, AppState>) -> AppResult<LiveSessionView> {
    // Compile without holding the live lock (same rule as `live_start`: the
    // lock is never held across an await).
    let service_id = {
        let guard = state.live.lock();
        guard
            .as_ref()
            .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?
            .service_id
            .clone()
    };
    let cue_list = CueCompiler::new(&state.db.pool)
        .compile(&service_id)
        .await?;

    let mut guard = state.live.lock();
    let session = guard
        .as_mut()
        .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
    if session.service_id != service_id {
        return Err(AppError::Validation(
            "live-sesjonen byttet tjeneste under rekompilering".into(),
        ));
    }
    session.replace_cue_list(cue_list, now_ms());
    let view = session.view();
    // The recovery WAL's header holds the cue list, so replaying old actions
    // against the new list would resume at the wrong cue. Rewrite the log:
    // fresh header (new list) + synthetic actions that reproduce the current
    // position and output override. Best-effort, like every store write.
    let snapshot = session.clone();
    drop(guard);
    let s = store(&state);
    let _ = s.begin(&snapshot);
    let _ = s.record(&LiveAction::GoTo {
        index: snapshot.index,
    });
    match snapshot.output {
        OutputState::Blackout => {
            let _ = s.record(&LiveAction::Blackout);
        }
        OutputState::Logo => {
            let _ = s.record(&LiveAction::ShowLogo);
        }
        OutputState::Message => {
            let _ = s.record(&LiveAction::ShowMessage {
                text: snapshot.message_text.clone().unwrap_or_default(),
            });
        }
        OutputState::Normal => {}
    }
    push_to_outputs(&state, &view.frame);
    Ok(view)
}

/// Apply one operator action to the running session.
#[tauri::command]
pub fn live_dispatch(state: State<'_, AppState>, action: LiveAction) -> AppResult<LiveSessionView> {
    let mut guard = state.live.lock();
    let session = guard
        .as_mut()
        .ok_or_else(|| AppError::Validation("ingen aktiv live-sesjon".into()))?;
    // Log the action before applying it; a failed append must not break the
    // show (worst case: recovery loses the last action).
    let _ = store(&state).record(&action);
    session.dispatch(action, now_ms());
    // Two relaxed `fetch_add`s, inside the live lock, on the hottest path in
    // the app. Nothing else about this command changed.
    state.telemetry.note_cue_dispatched();
    let view = session.view();
    // Phase 12.2 — best-effort companion broadcast of the new frame. The slide
    // carries its own `sensitive_slide` gate; a failed publish is logged and
    // never breaks the show (the companion is off the critical live path).
    drop(guard);
    let next_seq = {
        let mut comp = state.companion.lock();
        match comp.as_mut() {
            Some(broadcaster) => {
                // `seq` advances even if the publish fails (so a retry never
                // reuses it), so capture it regardless of the result.
                if broadcaster.on_cue_advance(&view.frame, false).is_err() {
                    // The transport's error string is NOT logged: once the
                    // cloud layer lands it is a network error carrying an
                    // endpoint, and this line reaches the uploadable log tail
                    // (law 2). The COUNT is the finding.
                    state.telemetry.note_companion_failure();
                    tracing::warn!(seam = "cue_advance", "companion broadcast failed");
                }
                Some(broadcaster.next_seq())
            }
            None => None,
        }
    };
    if let Some(seq) = next_seq {
        let _ = store(&state).record_seq(seq);
    }
    push_to_outputs(&state, &view.frame);
    Ok(view)
}

/// Snapshot of the current session, or `None` if not live.
#[tauri::command]
pub fn live_state(state: State<'_, AppState>) -> AppResult<Option<LiveSessionView>> {
    Ok(state.live.lock().as_ref().map(|s| s.view()))
}

/// End the session and clear the recovery log (marks a clean shutdown).
#[tauri::command]
pub async fn live_end(state: State<'_, AppState>) -> AppResult<()> {
    end_session(&state).await
}

/// The body of [`live_end`], over a plain `&AppState` so what the service
/// leaves behind — its quality row and its song usage — is provable without a
/// Tauri runtime.
async fn end_session(state: &AppState) -> AppResult<()> {
    // Phase 12.2 — tell phones the service is over, then tear down the
    // broadcaster. Best-effort: a failed publish must not block ending.
    if let Some(broadcaster) = state.companion.lock().as_mut() {
        if broadcaster.on_service_end().is_err() {
            state.telemetry.note_companion_failure();
            tracing::warn!(seam = "service_end", "companion broadcast failed");
        }
    }
    *state.companion.lock() = None;
    let ended_at = now_ms();
    // A7 — read the display record BEFORE the lock is cleared, for the same
    // reason: it belongs to the service that just ended. Pure and O(log
    // entries); the DB write happens further down, after the lock is gone.
    let ending = ending_session(state, ended_at);
    // E3 — build the session's quality row BEFORE the lock is cleared, so the
    // numbers belong to the service that just ended. Still a pure atomic
    // read-and-reset plus one bounded `try_send`.
    state
        .telemetry
        .finish_session(ended_at, SessionOutcome::Clean);
    *state.live.lock() = None;
    state.recovery_offer.store(false, Ordering::SeqCst);
    store(state).clear();
    // The outputs stay open (the operator closes them separately) but the
    // service is over — show black, never a stale slide.
    push_to_outputs(state, &LiveFrame::Black);
    // The songs the congregation actually sang, filed for the TONO/CCLI report.
    // Never on the live path: the service is over and the projector is black.
    file_song_usage(state, ending).await;
    // …and NOW — with `live` provably `None` — is the moment the buffered rows
    // and counters may touch a disk. The gate is inside `flush`, so this is
    // safe even if a new service goes live between these two statements.
    let _ = crate::commands::telemetry::flush(state).await;
    Ok(())
}

/// On launch, recover an abnormally-terminated session if one exists. Installs
/// it as the active session and returns its view so the UI can offer "resume".
///
/// **A running service is never replaced.** This command is called on every
/// mount of the operator workspace — including the remount the
/// [`ErrorBoundary`](../../../src/components/ErrorBoundary.tsx) performs
/// mid-service when a panel throws — and rebuilding the session from the WAL
/// there would restart the very service that is on the projector: a fresh
/// `started_at`, a re-zeroed action log, and a recovery banner offered over a
/// live congregation. When something is already live the answer is that
/// session, unchanged, and nothing else happens.
#[tauri::command]
pub fn live_recover(state: State<'_, AppState>) -> AppResult<Option<LiveSessionView>> {
    recover_session(&state)
}

/// The body of [`live_recover`], over a plain `&AppState` so the "never replace
/// a running service" rule is provable without a Tauri runtime.
fn recover_session(state: &AppState) -> AppResult<Option<LiveSessionView>> {
    if let Some(view) = state.live.lock().as_ref().map(|s| s.view()) {
        return Ok(Some(view));
    }
    let Some(session) = store(state).recover() else {
        return Ok(None);
    };
    let view = session.view();
    // Re-establish the companion broadcaster for the recovered service. Seed the
    // `seq` above any frame the crashed session could have broadcast so phones
    // still subscribed to `companion:<svc>` don't discard every post-recover
    // frame via their `seq <= lastSeq` stale-guard. Prefer the persisted seq —
    // it captures unlogged `companion_broadcast` re-pushes that the action-log
    // length misses — and floor it at `log_len` (which dispatches keep in sync)
    // in case the sidecar is absent or torn. Recovery never depends on the
    // crashed process's in-memory state.
    let resume_seq = store(state)
        .recover_seq()
        .unwrap_or(0)
        .max(view.log_len as u32);
    *state.companion.lock() = Some(CompanionBroadcaster::resuming(
        &view.service_id,
        RealtimeTransport::local_only(),
        resume_seq,
    ));
    // E3 — a resumed session is still a session: it gets its own quality row,
    // flagged `recovered`, when it eventually ends.
    state.telemetry.begin_session(view.started_at);
    state.telemetry.note_session_recovered();
    *state.live.lock() = Some(session);
    // Installed, but not yet ACCEPTED: the operator still has to answer the
    // banner. `live_discard_recovery` needs to know the difference between this
    // session and one the operator started (see there).
    state.recovery_offer.store(true, Ordering::SeqCst);
    push_to_outputs(state, &view.frame);
    Ok(Some(view))
}

/// Discard a crash-recovery offer — WITHOUT ending a service that is running.
///
/// The narrow command behind the recovery banner's "Discard". It used to be
/// `live_end`, which unconditionally pushes `Black` to the outputs: on the
/// mount that follows an `ErrorBoundary` reload the banner can appear over a
/// RUNNING service, and one click then blacked a live projector. Ending a
/// service is `live_end`'s job and stays there.
///
/// Two cases, and only the WAL is common to both:
///
///   * the operator answered a banner for a session they never accepted (the
///     ordinary launch case): `live_recover` installed it so the projector
///     could show where the crash left off, and nothing else in the app would
///     ever end it — while it sat there the telemetry drain's live gate stayed
///     shut. So it ends here, exactly as `live_end` would have ended it.
///   * a service is actually running: the WAL goes, the service does not.
#[tauri::command]
pub async fn live_discard_recovery(state: State<'_, AppState>) -> AppResult<()> {
    discard_recovery(&state).await
}

/// The body of [`live_discard_recovery`], over a plain `&AppState` so both of
/// its cases are provable without a Tauri runtime.
async fn discard_recovery(state: &AppState) -> AppResult<()> {
    // "Discard" means the log stops being an offer, in both cases.
    store(state).clear();
    if !state.recovery_offer.swap(false, Ordering::SeqCst) {
        // A service the operator started is on the projector. Nothing else.
        return Ok(());
    }
    if let Some(broadcaster) = state.companion.lock().as_mut() {
        if broadcaster.on_service_end().is_err() {
            state.telemetry.note_companion_failure();
            tracing::warn!(seam = "service_end", "companion broadcast failed");
        }
    }
    *state.companion.lock() = None;
    let ended_at = now_ms();
    // A7 — a recovered session is still a service that reached a congregation:
    // the crash is why nobody pressed "end", not evidence that nothing was sung.
    let ending = ending_session(state, ended_at);
    state
        .telemetry
        .finish_session(ended_at, SessionOutcome::Clean);
    *state.live.lock() = None;
    push_to_outputs(state, &LiveFrame::Black);
    file_song_usage(state, ending).await;
    let _ = crate::commands::telemetry::flush(state).await;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::models::{LibraryInput, SongInput};
    use crate::db::repositories::{LibraryRepo, TelemetryRepo};
    use crate::db::Database;
    use crate::services::cue_list::CueList;
    use crate::telemetry::quality::QualityCollector;
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// An `AppState` with a real in-memory database, a real data dir and
    /// nothing live. Everything these tests touch — the WAL, the collector, the
    /// recovery flag — is the production type, not a stand-in.
    pub(crate) async fn app_state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::open_in_memory().await.expect("db");
        (
            AppState {
                db,
                data_dir: dir.path().to_path_buf(),
                live: Mutex::new(None),
                recovery_offer: AtomicBool::new(false),
                companion: Mutex::new(None),
                outputs: Mutex::new(None),
                pending_update: Mutex::new(None),
                telemetry: Arc::new(QualityCollector::new()),
            },
            dir,
        )
    }

    fn a_session(service_id: &str, started_at: i64) -> LiveSession {
        LiveSession::new(
            service_id,
            CueList {
                service_id: service_id.into(),
                compiled_at: 0,
                cues: vec![],
            },
            started_at,
        )
    }

    /// A leftover crash WAL for `service_id`, as a previous process would have
    /// left it.
    fn a_leftover_wal(state: &AppState, service_id: &str) {
        let mut session = a_session(service_id, 1_000);
        store(state).begin(&session).expect("begin");
        session.dispatch(LiveAction::Next, 2_000);
        store(state).record(&LiveAction::Next).expect("record");
    }

    /// An empty service that compiles, so `start_session` can be driven for
    /// real rather than around.
    async fn a_service(state: &AppState) -> String {
        let library = LibraryRepo::new(&state.db.pool)
            .create(LibraryInput {
                name: "Menighet".into(),
                default_locale: None,
            })
            .await
            .expect("library");
        ServiceRepo::new(&state.db.pool)
            .create(&library.id, "Gudstjeneste", 0)
            .await
            .expect("service")
            .id
    }

    // ── the recovery banner must never reach a running service ──────────────

    /// Core promise #1, at the seam that could break it hardest.
    ///
    /// `live_recover` runs on every mount of the operator workspace, and the
    /// `ErrorBoundary` remounts the workspace mid-service by reloading the
    /// webview. Rebuilding the session from the WAL there replaced the running
    /// service with a copy of itself — new `started_at`, re-zeroed log — and
    /// raised a recovery banner over a live congregation whose Discard button
    /// then ran `live_end` and blacked the projector.
    #[tokio::test]
    async fn a_recovery_never_replaces_a_running_service() {
        let (state, _dir) = app_state().await;
        a_leftover_wal(&state, "crashed-service");
        *state.live.lock() = Some(a_session("todays-service", 999_000));

        let view = recover_session(&state)
            .expect("recovery is not an error")
            .expect("the running session is the answer");

        assert_eq!(
            view.service_id, "todays-service",
            "the answer is the RUNNING service, never the WAL's"
        );
        let live = state.live.lock();
        let running = live.as_ref().expect("still live");
        assert_eq!(running.service_id, "todays-service");
        assert_eq!(
            running.started_at, 999_000,
            "the running session was not rebuilt underneath the operator"
        );
        assert!(
            !state.recovery_offer.load(Ordering::SeqCst),
            "a running service is never an unanswered offer"
        );
        // …and the WAL is untouched: it is still the crash record it was.
        assert!(store(&state).exists());
    }

    /// Discard, over a running service, must not touch the projector.
    #[tokio::test]
    async fn discarding_over_a_running_service_leaves_the_service_running() {
        let (state, _dir) = app_state().await;
        a_leftover_wal(&state, "crashed-service");
        *state.live.lock() = Some(a_session("todays-service", 999_000));
        state.telemetry.begin_session(999_000);

        discard_recovery(&state).await.expect("discard");

        let live = state.live.lock();
        assert!(
            live.is_some(),
            "the service the operator started is still live — nothing was blacked"
        );
        assert_eq!(live.as_ref().expect("live").service_id, "todays-service");
        drop(live);
        assert!(!store(&state).exists(), "the offer itself is gone");
    }

    /// The ordinary launch case still ends the offer it discards — nothing else
    /// in the app ever would, and while it sat in `live` the telemetry drain's
    /// gate stayed shut.
    #[tokio::test]
    async fn discarding_an_unanswered_offer_ends_it() {
        let (state, _dir) = app_state().await;
        a_leftover_wal(&state, "crashed-service");

        recover_session(&state).expect("recover").expect("an offer");
        assert!(state.recovery_offer.load(Ordering::SeqCst));
        assert!(state.live.lock().is_some(), "installed for the projector");

        discard_recovery(&state).await.expect("discard");

        assert!(state.live.lock().is_none(), "the offer was ended");
        assert!(!state.recovery_offer.load(Ordering::SeqCst));
        assert!(!store(&state).exists());
        // The gate is open again, and the discarded session's row went to disk
        // through the flush the discard performs.
        assert_eq!(
            TelemetryRepo::new(&state.db.pool)
                .quality_count()
                .await
                .expect("count"),
            1
        );
    }

    // ── one service's numbers never belong to another's row ─────────────────

    /// The other half of the rehearsal bug: `begin_session` resetting the
    /// counters is only honest if the session it replaces got its row FIRST.
    /// Otherwise the rehearsal simply disappears — no row, no cues, no trace of
    /// the output restart it hit.
    #[tokio::test]
    async fn going_live_again_gives_the_replaced_session_its_own_row() {
        let (state, _dir) = app_state().await;
        let service = a_service(&state).await;

        // The 09:40 rehearsal: forty cues and an output child that died.
        start_session(&state, service.clone()).await.expect("live");
        for _ in 0..40 {
            state.telemetry.note_cue_dispatched();
        }
        state.telemetry.note_output_restart();

        // 11:00, without a `live_end` in between.
        start_session(&state, service).await.expect("live again");
        state.telemetry.note_cue_dispatched();

        let rows = state.telemetry.take_buffered_rows();
        let rehearsal = rows.last().expect("the replaced session kept its row");
        assert_eq!(rehearsal.cue_count, 40, "{rehearsal:?}");
        assert_eq!(rehearsal.output_child_restarts, 1, "{rehearsal:?}");

        // …and the service that follows starts from zero.
        state
            .telemetry
            .finish_session(now_ms(), SessionOutcome::Clean);
        let service_row = state
            .telemetry
            .take_buffered_rows()
            .pop()
            .expect("the service's row");
        assert_eq!(service_row.cue_count, 1, "{service_row:?}");
        assert_eq!(service_row.output_child_restarts, 0, "{service_row:?}");
    }

    // ── A7 — sangbruksloggen, gjennom de ekte kommandoene ───────────────────

    /// A service with two songs on the plan. Returns (service_id, [song ids]).
    async fn a_service_with_songs(state: &AppState, titles: &[&str]) -> (String, Vec<String>) {
        let library = LibraryRepo::new(&state.db.pool)
            .create(LibraryInput {
                name: "Menighet".into(),
                default_locale: None,
            })
            .await
            .expect("library");
        let service_repo = ServiceRepo::new(&state.db.pool);
        let service = service_repo
            .create(&library.id, "Gudstjeneste", 0)
            .await
            .expect("service");
        let song_repo = SongRepo::new(&state.db.pool);
        let mut ids = Vec::new();
        for (position, title) in titles.iter().enumerate() {
            let song = song_repo
                .create(SongInput {
                    library_id: library.id.clone(),
                    title: (*title).into(),
                    language: None,
                    default_key: None,
                    tempo_bpm: None,
                    ccli_song_id: Some(format!("ccli-{position}")),
                    tono_work_id: None,
                    copyright_notice: Some("© 2020 Menigheten".into()),
                })
                .await
                .expect("song");
            song_repo
                .add_section(&song.id, "verse_1", "Første linje\nAndre linje")
                .await
                .expect("section");
            service_repo
                .add_item(
                    &service.id,
                    position as i64,
                    "song",
                    Some(&song.id),
                    None,
                    None,
                    None,
                    None,
                )
                .await
                .expect("item");
            ids.push(song.id);
        }
        (service.id, ids)
    }

    fn usage(state: &AppState) -> SongUsageRepo<'_> {
        SongUsageRepo::new(&state.db.pool)
    }

    /// The whole seam, end to end: go live, sing the first song, end the
    /// service — and the log holds one honest row with the licensing metadata
    /// the report needs.
    #[tokio::test]
    async fn en_ekte_gudstjeneste_gir_en_loggrad_med_lisensdata() {
        let (state, _dir) = app_state().await;
        let (service, songs) =
            a_service_with_songs(&state, &["Åpningssang", "Slutningssang"]).await;

        start_session(&state, service).await.expect("live");
        // Backdate the start so the first cue provably held the output long
        // enough — the wall clock in a test runs in microseconds.
        if let Some(s) = state.live.lock().as_mut() {
            s.started_at -= 300_000;
        }
        end_session(&state).await.expect("end");

        let rows = usage(&state).list_between(0, i64::MAX).await.expect("list");
        assert_eq!(rows.len(), 1, "bare sangen som sto på skjermen: {rows:?}");
        assert_eq!(rows[0].title, "Åpningssang");
        assert_eq!(rows[0].song_id, songs[0]);
        assert_eq!(rows[0].ccli_song_id.as_deref(), Some("ccli-0"));
        assert_eq!(
            rows[0].copyright_notice.as_deref(),
            Some("© 2020 Menigheten")
        );
        assert_eq!(rows[0].service_name, "Gudstjeneste");
        assert_eq!(rows[0].show_count, 1);
        assert!(rows[0].visible_ms >= 300_000, "{:?}", rows[0]);
    }

    /// A service nobody actually ran — go live, notice it's the wrong plan,
    /// stop — must leave the report untouched. The threshold is what makes
    /// "faktisk brukt" mean it.
    #[tokio::test]
    async fn en_okt_som_ble_avbrutt_med_en_gang_gir_ingen_rad() {
        let (state, _dir) = app_state().await;
        let (service, _) = a_service_with_songs(&state, &["Åpningssang"]).await;

        start_session(&state, service).await.expect("live");
        end_session(&state).await.expect("end");

        assert_eq!(
            usage(&state).count().await.expect("count"),
            0,
            "et sekund på skjermen er ikke bruk"
        );
    }

    /// The 09:40 rehearsal and the 11:00 service on the same plan are ONE use.
    /// `start_session` files the session it replaces, and the writer merges it
    /// with the service that follows.
    #[tokio::test]
    async fn generalprove_og_gudstjeneste_samme_dag_blir_en_rad() {
        let (state, _dir) = app_state().await;
        let (service, _) = a_service_with_songs(&state, &["Åpningssang"]).await;

        // 09:40 — the rehearsal, sung through.
        start_session(&state, service.clone()).await.expect("live");
        if let Some(s) = state.live.lock().as_mut() {
            s.started_at -= 300_000;
        }
        // 11:00, without a `live_end` in between: this is what files the
        // rehearsal.
        start_session(&state, service).await.expect("live again");
        if let Some(s) = state.live.lock().as_mut() {
            s.started_at -= 300_000;
        }
        end_session(&state).await.expect("end");

        let rows = usage(&state).list_between(0, i64::MAX).await.expect("list");
        assert_eq!(rows.len(), 1, "én rad, ikke to: {rows:?}");
        assert_eq!(rows[0].show_count, 2, "begge gangene er synlige");
    }

    /// A crash-recovered session that the operator discards is still a service
    /// that reached a congregation — the crash is why nobody pressed "end".
    #[tokio::test]
    async fn en_gjenopprettet_okt_blir_ogsa_fort_i_loggen() {
        let (state, _dir) = app_state().await;
        let (service, _) = a_service_with_songs(&state, &["Åpningssang"]).await;

        start_session(&state, service).await.expect("live");
        if let Some(s) = state.live.lock().as_mut() {
            s.started_at -= 300_000;
        }
        // The process "crashed": the WAL is on disk and the session is gone.
        let crashed = state.live.lock().take().expect("live");
        store(&state).begin(&crashed).expect("wal");
        state.recovery_offer.store(false, Ordering::SeqCst);

        recover_session(&state).expect("recover").expect("an offer");
        discard_recovery(&state).await.expect("discard");

        assert_eq!(usage(&state).count().await.expect("count"), 1);
    }
}
