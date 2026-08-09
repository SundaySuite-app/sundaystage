//! E2 — the update ring (channel) the app updates from.
//!
//! Up to v0.4.0 SundayStage polled the GitHub Releases `latest.json` for the
//! `latest` release. From v0.5.0 the app polls the shared Sunday update Worker
//! instead, on an **app-scoped ring**:
//!
//! ```text
//! https://updates.sundaysuite.app/v1/update/sundaystage/stable
//! https://updates.sundaysuite.app/v1/update/sundaystage/beta
//! ```
//!
//! The ring answers with the ordinary Tauri updater manifest (same JSON shape
//! as the GitHub `latest.json`, signed with the *same* key — the pubkey in
//! `tauri.conf.json` is unchanged), or **204 No Content** when nothing is
//! promoted to that ring / the ring is paused. 204 is not an error: it means
//! "you are up to date", and `tauri-plugin-updater` already returns `Ok(None)`
//! for it (see the direction/204 tests at the bottom of this file). A ring that
//! does not exist answers 404.
//!
//! Why a *runtime* endpoint and not just `tauri.conf.json`: the operator can
//! opt into the beta ring from Settings, and the plugin's JS `check()` has no
//! `endpoints` option (its `CheckOptions` as of plugin 2.10.1 =
//! headers/timeout/proxy/target/allowDowngrades) and the Rust plugin `Builder`
//! has no `endpoints()` either. The only seam that can pick an endpoint at
//! runtime is `UpdaterBuilder::endpoints(..)`, which is why the update check
//! moved into Rust (`commands::updater`). **This file is the single source of
//! truth for the channel**; `tauri.conf.json` keeps the stable ring as the
//! configured fallback, pinned equal by a test below.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tiny JSON flag file in the app-local data dir, same shape as
/// `services::crash`'s opt-in flag: `{"channel":"stable"}`.
const SETTINGS_FILE: &str = "update_channel.json";

/// App-scoped ring base on the shared Sunday update Worker. The `{app}`
/// segment (`sundaystage`) is part of the Worker's app registry — never reuse
/// SundayRec's frozen `/v1/update/{channel}` aliases from here.
pub const RING_BASE: &str = "https://updates.sundaysuite.app/v1/update/sundaystage";

/// Which update ring this install follows.
///
/// Defaults to [`UpdateChannel::Stable`] — an unreadable, missing or malformed
/// settings file resolves to stable, never to beta (an operator must opt in
/// deliberately; a corrupted file must not silently move a church onto betas).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/UpdateChannel.ts")]
pub enum UpdateChannel {
    /// Promoted, generally available builds. The default.
    #[default]
    Stable,
    /// Gets promoted builds earlier, may have rough edges.
    Beta,
}

impl UpdateChannel {
    /// The ring segment used in the Worker URL.
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateChannel::Stable => "stable",
            UpdateChannel::Beta => "beta",
        }
    }

    /// The full updater endpoint for this ring.
    pub fn endpoint(self) -> String {
        format!("{RING_BASE}/{}", self.as_str())
    }
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}

/// The persisted channel, or [`UpdateChannel::Stable`] on a fresh/garbled file.
pub fn load(data_dir: &Path) -> UpdateChannel {
    std::fs::read_to_string(settings_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("channel")
                .and_then(|c| serde_json::from_value::<UpdateChannel>(c.clone()).ok())
        })
        .unwrap_or_default()
}

/// Persist the operator's channel choice.
pub fn save(data_dir: &Path, channel: UpdateChannel) -> std::io::Result<()> {
    let body = serde_json::json!({ "channel": channel }).to_string();
    std::fs::write(settings_path(data_dir), body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;
    use tauri_plugin_updater::RemoteRelease;

    #[test]
    fn endpoints_are_the_app_scoped_rings() {
        assert_eq!(
            UpdateChannel::Stable.endpoint(),
            "https://updates.sundaysuite.app/v1/update/sundaystage/stable"
        );
        assert_eq!(
            UpdateChannel::Beta.endpoint(),
            "https://updates.sundaysuite.app/v1/update/sundaystage/beta"
        );
    }

    /// `tauri.conf.json` is the fallback the plugin uses if anything ever
    /// checks without our runtime override. It must point at the *stable* ring
    /// — and at exactly one endpoint, so a stray second entry can't quietly
    /// resurrect the GitHub path.
    #[test]
    fn tauri_conf_endpoint_is_the_stable_ring() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let endpoints = conf["plugins"]["updater"]["endpoints"].as_array().unwrap();
        assert_eq!(endpoints.len(), 1, "exactly one configured endpoint");
        assert_eq!(endpoints[0], UpdateChannel::Stable.endpoint());
    }

    #[test]
    fn defaults_to_stable_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), UpdateChannel::Stable); // fresh machine

        save(dir.path(), UpdateChannel::Beta).unwrap();
        assert_eq!(load(dir.path()), UpdateChannel::Beta);

        // …and back again: beta is never a one-way door.
        save(dir.path(), UpdateChannel::Stable).unwrap();
        assert_eq!(load(dir.path()), UpdateChannel::Stable);
    }

    #[test]
    fn garbled_or_unknown_channel_resolves_to_stable() {
        let dir = tempfile::tempdir().unwrap();
        for body in [
            "not json at all",
            "{}",
            r#"{"channel":"nightly"}"#,
            r#"{"channel":null}"#,
            r#"{"enabled":true}"#,
        ] {
            std::fs::write(settings_path(dir.path()), body).unwrap();
            assert_eq!(
                load(dir.path()),
                UpdateChannel::Stable,
                "corrupt settings must never move an install onto beta: {body}"
            );
        }
    }

    #[test]
    fn channel_serialises_as_the_ring_segment() {
        // The wire value the UI sends/receives must equal the URL segment, so
        // there is only one spelling of "beta" in the whole system.
        for c in [UpdateChannel::Stable, UpdateChannel::Beta] {
            assert_eq!(serde_json::to_value(c).unwrap(), c.as_str());
        }
    }

    // ── Update-direction pin ────────────────────────────────────────────────
    //
    // We do NOT hand-roll a version comparator. `tauri-plugin-updater` decides
    // with its default rule
    //
    //     release.version > self.current_version          (semver::Version Ord)
    //
    // — tauri-plugin-updater-2.10.1/src/updater.rs:530-533, reached because we
    // never call `UpdaterBuilder::version_comparator`. The test below runs that
    // exact expression over the plugin's own `RemoteRelease` deserializer, fed
    // manifests in the shape the ring serves, so the direction is pinned for
    // the versions this program ships.
    //
    // (SundayRec's E7 shipped a *custom* comparator that stripped the
    // prerelease and answered `is_newer("0.11.0", "0.11.0-beta.1") == false`.
    // Deferring to semver's Ord — where a prerelease sorts *below* its release
    // — is precisely how that class of bug is avoided here.)

    /// A ring manifest in the static/`platforms` shape the Worker serves.
    fn manifest(version: &str) -> String {
        format!(
            r#"{{
                 "version": "{version}",
                 "notes": "test",
                 "pub_date": "2026-08-08T12:00:00Z",
                 "platforms": {{
                   "darwin-aarch64": {{
                     "signature": "sig",
                     "url": "https://example.invalid/SundayStage.app.tar.gz"
                   }}
                 }}
               }}"#
        )
    }

    /// Exactly what the plugin does with a 200 manifest: parse it, then apply
    /// the default comparator against the running version.
    fn is_update(current: &str, remote: &str) -> bool {
        let release: RemoteRelease = serde_json::from_str(&manifest(remote))
            .unwrap_or_else(|e| panic!("ring manifest for {remote} must parse: {e}"));
        release.version > Version::parse(current).unwrap()
    }

    #[test]
    fn update_direction_for_the_versions_this_program_ships() {
        // The stable hop this stage ships.
        assert!(is_update("0.4.0", "0.5.0"));
        // A beta tester on the beta ring must be able to land on the release.
        assert!(is_update("0.6.0-beta.1", "0.6.0"));
        // …but a stable install must NEVER be pulled down onto a prerelease.
        assert!(!is_update("0.6.0", "0.6.0-beta.1"));
        // Same version = nothing to do (the ring may still answer 200).
        assert!(!is_update("0.5.0", "0.5.0"));
        // Within the beta ring, betas advance in order.
        assert!(is_update("0.6.0-beta.1", "0.6.0-beta.2"));
        assert!(!is_update("0.6.0-beta.2", "0.6.0-beta.1"));
        // And a beta is still ahead of the previous stable.
        assert!(is_update("0.5.0", "0.6.0-beta.1"));
        // No downgrades: we never pass `allowDowngrades`/a comparator.
        assert!(!is_update("0.5.0", "0.4.0"));
    }

    /// The 204 contract, documented at the seam that consumes it.
    ///
    /// `tauri-plugin-updater` handles it *before* any parsing:
    ///
    /// ```text
    /// if StatusCode::NO_CONTENT == res.status() {
    ///     log::debug!("update endpoint returned 204 No Content");
    ///     return Ok(None);
    /// };
    /// ```
    /// (tauri-plugin-updater-2.10.1/src/updater.rs:481-485)
    ///
    /// So "nothing promoted / ring paused" surfaces as `Ok(None)` — the same
    /// value as "already newest" — and `commands::updater::update_check` maps
    /// both to `null`, i.e. *up to date*, never an error banner. There is no
    /// HTTP seam to drive from a unit test without a live server, so the pin
    /// here is on the empty body: a 204 carries no manifest, and an empty body
    /// is not a parseable release (which is why the plugin must, and does,
    /// short-circuit on the status code rather than on the body).
    #[test]
    fn a_204_body_is_not_a_release() {
        assert!(serde_json::from_str::<RemoteRelease>("").is_err());
    }
}
