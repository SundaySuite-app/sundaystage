//! Live-engine commands.
//!
//! Phase 5.1: compile a Service into a CueList.
//! Phase 5.3: drive the running [`LiveSession`] — start, dispatch operator
//! actions, snapshot, end. The session is held in `AppState` behind a mutex and
//! persisted to disk after every action for crash recovery (the Phase 5.2
//! output process independently holds the last frame if the UI dies).

use tauri::State;

use crate::db::now_ms;
use crate::db::repositories::{ServiceRepo, SongRepo};
use crate::error::{AppError, AppResult};
use crate::services::companion::transport::{CompanionBroadcaster, RealtimeTransport};
use crate::services::cue_list::{CueCompiler, CueList};
use crate::services::live_session::{LiveAction, LiveFrame, LiveSession, LiveSessionView};
use crate::services::session_store::SessionStore;
use crate::services::stage_display::{builtin_stage_presets, StageDisplayConfig};
use crate::services::sundayrec_bridge::export::{chapter_markers, session_to_srt, ChapterMarker};
use crate::services::sundayrec_bridge::manifest::{build_manifest, ItemMeta, ManifestSong};
use crate::services::sundayrec_bridge::protocol::PROTOCOL_VERSION;
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

/// Compile the service and start a live session (replacing any previous one).
#[tauri::command]
pub async fn live_start(
    state: State<'_, AppState>,
    service_id: String,
) -> AppResult<LiveSessionView> {
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
    let _ = store(&state).begin(&session);
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
    let _ = store(&state).record_seq(start_seq);
    *state.live.lock() = Some(session);
    push_to_outputs(&state, &view.frame);
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
                if let Err(e) = broadcaster.on_cue_advance(&view.frame, false) {
                    tracing::warn!("companion broadcast failed: {e}");
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
pub fn live_end(state: State<'_, AppState>) -> AppResult<()> {
    // Phase 12.2 — tell phones the service is over, then tear down the
    // broadcaster. Best-effort: a failed publish must not block ending.
    if let Some(broadcaster) = state.companion.lock().as_mut() {
        if let Err(e) = broadcaster.on_service_end() {
            tracing::warn!("companion service-end broadcast failed: {e}");
        }
    }
    *state.companion.lock() = None;
    *state.live.lock() = None;
    store(&state).clear();
    // The outputs stay open (the operator closes them separately) but the
    // service is over — show black, never a stale slide.
    push_to_outputs(&state, &LiveFrame::Black);
    Ok(())
}

/// On launch, recover an abnormally-terminated session if one exists. Installs
/// it as the active session and returns its view so the UI can offer "resume".
#[tauri::command]
pub fn live_recover(state: State<'_, AppState>) -> AppResult<Option<LiveSessionView>> {
    let Some(session) = store(&state).recover() else {
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
    let resume_seq = store(&state)
        .recover_seq()
        .unwrap_or(0)
        .max(view.log_len as u32);
    *state.companion.lock() = Some(CompanionBroadcaster::resuming(
        &view.service_id,
        RealtimeTransport::local_only(),
        resume_seq,
    ));
    *state.live.lock() = Some(session);
    push_to_outputs(&state, &view.frame);
    Ok(Some(view))
}
