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
use crate::telemetry::counters::CounterName;
use crate::telemetry::quality::LiveSafe;
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

/// A verified update, remembered together with the ring it came from.
///
/// The pair is the point. An offer is only valid for the ring it was fetched
/// from, and the operator can change rings while the banner is on screen — so
/// the channel travels with the update rather than being re-read (and
/// re-trusted) at install time.
///
/// Generic in the payload only so the rules below can be tested: a
/// `tauri_plugin_updater::Update` has private fields and cannot be built
/// outside the plugin, and a rule about update rings that is only exercised by
/// downloading a real signed build is a rule nothing checks.
#[derive(Clone)]
pub struct Offer<T> {
    pub channel: UpdateChannel,
    pub update: T,
}

/// What `AppState` holds.
pub type PendingUpdate = Offer<tauri_plugin_updater::Update>;

/// What a standing offer is worth to the ring the install follows NOW.
#[derive(Debug, PartialEq, Eq)]
enum OfferState<T> {
    /// Same ring: install exactly what the operator was shown.
    Ready(T),
    /// No check has found anything in this session.
    Nothing,
    /// The offer came from a ring this install no longer follows. It is dropped
    /// as it is refused — the banner must not get a second chance.
    WrongRing,
}

/// Take the standing offer if, and only if, it still belongs to `following`.
///
/// The guard never outlives this function, so the caller can await the download
/// afterwards without holding a lock across it.
fn take_offer<T: Clone>(
    pending: &parking_lot::Mutex<Option<Offer<T>>>,
    following: UpdateChannel,
) -> OfferState<T> {
    let mut guard = pending.lock();
    match guard.as_ref() {
        None => OfferState::Nothing,
        Some(offer) if offer.channel == following => OfferState::Ready(offer.update.clone()),
        Some(_) => {
            *guard = None;
            OfferState::WrongRing
        }
    }
}

/// Persist the ring choice and drop any standing offer.
fn set_channel<T>(
    data_dir: &std::path::Path,
    pending: &parking_lot::Mutex<Option<Offer<T>>>,
    channel: UpdateChannel,
) -> AppResult<UpdateChannel> {
    update_channel::save(data_dir, channel)?;
    *pending.lock() = None;
    Ok(channel)
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
///
/// **The standing offer is dropped.** Leaving it in place meant an operator who
/// opted OUT of the beta ring could still install the beta from the banner that
/// was already on screen — one click, straight back onto the build they had
/// just decided against, days before the next stable check.
#[tauri::command]
pub fn update_channel_set(
    state: State<'_, AppState>,
    channel: UpdateChannel,
) -> AppResult<UpdateChannel> {
    set_channel(&state.data_dir, &state.pending_update, channel)
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
    let found = check_ring(&app, channel).await;
    // Whatever the answer, the previous offer does not survive this check —
    // including when the check FAILED. A stale pending update is an install
    // button that works for a ring we could not reach and a version we could
    // not re-verify.
    let update = match found {
        Ok(update) => update,
        Err(e) => {
            *state.pending_update.lock() = None;
            return Err(e);
        }
    };

    let Some(update) = update else {
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
    // Hold the verified update — with its ring — so `update_install` installs
    // exactly what the operator was shown, without a second round-trip.
    *state.pending_update.lock() = Some(Offer { channel, update });
    Ok(Some(info))
}

/// Ask one ring for a newer signed build. Split out so the command above has a
/// single fallible expression to fold the "clear the offer" rule into.
async fn check_ring(
    app: &AppHandle,
    channel: UpdateChannel,
) -> AppResult<Option<tauri_plugin_updater::Update>> {
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
    updater.check().await.map_err(updater_error)
}

/// Download + install the update most recently offered by [`update_check`].
///
/// The ring is re-asserted first: the offer names the ring it came from, and if
/// this install no longer follows that ring the offer is refused and dropped
/// rather than installed. `update_channel_set` already clears the offer, so
/// this is the second lock on the same door — the one that still holds if the
/// channel file is changed by anything else.
///
/// The caller relaunches afterwards (`@tauri-apps/plugin-process`), matching
/// the pre-E2 behaviour.
#[tauri::command]
pub async fn update_install(state: State<'_, AppState>) -> AppResult<()> {
    // Cloned out of the lock inside `take_offer`: the guard must never live
    // across the await below.
    let update = match take_offer(&state.pending_update, update_channel::load(&state.data_dir)) {
        OfferState::Ready(update) => update,
        OfferState::Nothing => {
            return Err(AppError::Validation(
                "no update has been checked for in this session".into(),
            ))
        }
        OfferState::WrongRing => {
            return Err(AppError::Validation(
                "this update came from a ring the install no longer follows".into(),
            ))
        }
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(updater_error)?;
    // E3 — counted only on a SUCCESSFUL install, because the question this one
    // answers is "did the ring actually land", and a failed download did not.
    state.telemetry.note_counter(CounterName::UpdateInstalled);
    // …and persisted IMMEDIATELY: the caller relaunches the app right after
    // this returns, and an in-memory counter does not survive that. The live
    // gate is inside `flush`, so this is safe even mid-service.
    let _ = crate::commands::telemetry::flush(&state).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    /// A stand-in for the plugin's `Update`, which has private fields and can
    /// only be built by a real signed check. The rules under test are about the
    /// RING an offer belongs to, and those do not depend on the payload.
    type TestOffer = Mutex<Option<Offer<&'static str>>>;

    fn offer(channel: UpdateChannel) -> TestOffer {
        Mutex::new(Some(Offer {
            channel,
            update: "0.6.0-beta.1",
        }))
    }

    /// Opting out of beta must take the beta offer with it.
    ///
    /// The banner is already on screen when the operator changes rings in
    /// Settings. Leaving the offer in place left one click that installed the
    /// exact build they had just decided against — and `update_check` only runs
    /// at launch, so the stale button survived until the next restart.
    #[test]
    fn leaving_a_ring_drops_that_ring_s_offer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pending = offer(UpdateChannel::Beta);

        let now = set_channel(dir.path(), &pending, UpdateChannel::Stable).expect("saved");

        assert_eq!(now, UpdateChannel::Stable);
        assert_eq!(update_channel::load(dir.path()), UpdateChannel::Stable);
        assert!(
            pending.lock().is_none(),
            "the beta offer must not outlive the opt-out"
        );
    }

    /// …and the install path re-asserts the ring itself, so the rule still
    /// holds if the channel file changes by any other route.
    #[test]
    fn an_offer_from_another_ring_is_refused_and_dropped() {
        let pending = offer(UpdateChannel::Beta);

        assert_eq!(
            take_offer(&pending, UpdateChannel::Stable),
            OfferState::WrongRing
        );
        assert!(
            pending.lock().is_none(),
            "a refused offer is dropped, not left for a second click"
        );
        // …and with nothing standing, the answer is "nothing", not a refusal.
        assert_eq!(
            take_offer(&pending, UpdateChannel::Stable),
            OfferState::Nothing
        );
    }

    #[test]
    fn an_offer_from_the_ring_the_install_follows_installs() {
        let pending = offer(UpdateChannel::Beta);
        assert_eq!(
            take_offer(&pending, UpdateChannel::Beta),
            OfferState::Ready("0.6.0-beta.1")
        );
        assert!(
            pending.lock().is_some(),
            "a valid offer survives the install attempt — a failed download \
             must be retryable without a second round-trip to the ring"
        );
    }
}
