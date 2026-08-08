//! E2 — auto-update over the app-scoped rings.
//!
//! The check runs **in Rust** because that is the only seam where the endpoint
//! can be chosen at runtime: `UpdaterBuilder::endpoints(..)`. Neither the JS
//! `check()` options nor the Rust plugin `Builder` can override the endpoints
//! configured in `tauri.conf.json` (see `services::update_channel` for the
//! full note). The frontend therefore drives these commands instead of
//! `@tauri-apps/plugin-updater`'s JS API; everything else — signature
//! verification against the embedded pubkey, download, install, the Windows
//! installer args — is the plugin's own code, unchanged.
//!
//! `update_check` returns `Ok(None)` for "up to date". That covers both
//! "already newest" and the ring answering **204 No Content** (nothing
//! promoted / ring paused), which the plugin turns into `Ok(None)` before any
//! parsing. Neither is an error.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::services::update_channel::{self, UpdateChannel};
use crate::AppState;

/// What the operator is offered when a newer signed build exists on the ring.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/UpdateInfo.ts")]
pub struct UpdateInfo {
    /// Version available on the ring.
    pub version: String,
    /// Version currently running.
    pub current_version: String,
    /// Ring this manifest came from.
    pub channel: UpdateChannel,
    /// Release notes from the manifest, if any.
    pub notes: Option<String>,
}

fn updater_error(e: tauri_plugin_updater::Error) -> AppError {
    AppError::Internal(format!("updater: {e}"))
}

/// The ring this install follows.
#[tauri::command]
pub fn update_channel_get(state: State<'_, AppState>) -> UpdateChannel {
    update_channel::load(&state.data_dir)
}

/// Move this install to another ring. Takes effect on the next check — the
/// endpoint is resolved per check, not at startup.
#[tauri::command]
pub fn update_channel_set(
    state: State<'_, AppState>,
    channel: UpdateChannel,
) -> AppResult<UpdateChannel> {
    update_channel::save(&state.data_dir, channel)?;
    Ok(channel)
}

/// Check the current ring for a newer signed build.
///
/// `Ok(None)` = up to date (including a 204 from a paused/empty ring).
/// `Err(..)` = the check itself failed (offline, DNS, malformed manifest); the
/// frontend treats that as "no update" and stays quiet.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<UpdateInfo>> {
    let channel = update_channel::load(&state.data_dir);
    let endpoint = channel
        .endpoint()
        .parse()
        .map_err(|e| AppError::Internal(format!("updater endpoint: {e}")))?;

    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(updater_error)?
        .build()
        .map_err(updater_error)?;

    let found = updater.check().await.map_err(updater_error)?;

    let Some(update) = found else {
        // Up to date, or the ring answered 204.
        *state.pending_update.lock() = None;
        return Ok(None);
    };

    let info = UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        channel,
        notes: update.body.clone(),
    };
    // Hold the verified update so `update_install` installs exactly what the
    // operator was shown, without a second round-trip to the ring.
    *state.pending_update.lock() = Some(update);
    Ok(Some(info))
}

/// Download + install the update most recently offered by [`update_check`].
///
/// The caller relaunches afterwards (`@tauri-apps/plugin-process`), matching
/// the pre-E2 behaviour.
#[tauri::command]
pub async fn update_install(state: State<'_, AppState>) -> AppResult<()> {
    // Clone out of the lock first: the guard must never live across the await.
    let pending = state.pending_update.lock().clone();
    let update = pending.ok_or_else(|| {
        AppError::Validation("no update has been checked for in this session".into())
    })?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(updater_error)?;
    Ok(())
}
