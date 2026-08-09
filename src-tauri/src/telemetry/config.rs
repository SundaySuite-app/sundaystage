//! E5 — where the telemetry endpoint lives, and the key used to write to it.
//!
//! Both halves are baked in at BUILD time with `option_env!`. `None` means "this
//! build has no telemetry endpoint", and then no sender is ever constructed:
//! payloads keep queueing locally and nothing reaches the network. **Every dev
//! build and every CI run is in exactly that state**, which is the strongest
//! available form of "nothing sends anywhere while this stage is in flight".
//! E6's release job sets the two variables with `gh secret`.
//!
//! ## Why the write key is not a secret, and why it still exists
//!
//! It ships inside every SundayStage binary. Anyone with a copy of the app and a
//! hex editor has it, and no amount of obfuscation changes that. So it is not a
//! defence and is not treated as one: it is a spam filter that stops the
//! endpoint being an open write target for anything that finds the URL, and
//! nothing more. Every guarantee that matters — that a payload cannot carry a
//! path, a song title, a church name, or an unknown field — is enforced
//! server-side by the Worker's validator, which trusts no caller.
//!
//! What it must NOT be is the key that reads data back out. It is not: the
//! admin summary is behind a separate key that never leaves the owner's password
//! manager, so a key extracted from a binary cannot read the fleet.
//!
//! ## Why a missing key is not "send anyway"
//!
//! [`TelemetryEndpoint::resolve`] requires BOTH. A build with a URL and no key
//! would post and collect a 401 for every payload — and a 401 is classified
//! PERMANENT, so each one would be dropped. Requiring both means such a build
//! simply has no sender, which is the quiet, correct failure.
//!
//! ## Divergence from SundayRec
//!
//! Rec reads a RUNTIME `std::env::var` first and falls back to the baked value.
//! Stage does not: a runtime variable means a shipped app can be pointed at an
//! arbitrary host by anything that can set an environment variable for it, and
//! the only thing it bought Rec was convenience in the ignored live tests. Those
//! still work here — `option_env!` is tracked in cargo's dep-info, so
//! `SUNDAYSTAGE_TELEMETRY_URL=… cargo test -- --ignored` rebuilds with the value
//! baked in.

/// The app segment in the Worker's app registry. Never SundayRec's frozen
/// `/v1/ingest` alias — that one belongs to a live fleet (law 4).
pub const APP_ID: &str = "sundaystage";

/// The build-time variable naming the endpoint's BASE url (no trailing path).
pub const BASE_URL_VAR: &str = "SUNDAYSTAGE_TELEMETRY_URL";

/// The build-time variable carrying the write key.
pub const WRITE_KEY_VAR: &str = "SUNDAYSTAGE_TELEMETRY_WRITE_KEY";

/// The header the Worker reads the write key from. Generic across apps since
/// E1 — Rec's `x-sundayrec-key` is a frozen alias and is not used from here.
pub const WRITE_KEY_HEADER: &str = "x-write-key";

/// The resolved telemetry endpoint for this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryEndpoint {
    /// The ingest URL, e.g.
    /// `https://telemetry.sundaysuite.app/v1/apps/sundaystage/ingest`.
    pub ingest_url: String,
    /// The base URL, for the deletion route.
    pub base_url: String,
    /// The static write key, sent as [`WRITE_KEY_HEADER`].
    pub write_key: String,
}

impl TelemetryEndpoint {
    /// Resolve from the values baked in at build time. `None` when either is
    /// missing or blank — and then there is no sender at all.
    pub fn resolve() -> Option<Self> {
        Self::normalize(
            option_env!("SUNDAYSTAGE_TELEMETRY_URL").map(str::to_string),
            option_env!("SUNDAYSTAGE_TELEMETRY_WRITE_KEY").map(str::to_string),
        )
    }

    /// Pure construction, so the rules are testable without a build variable.
    ///
    /// Rejects a non-`http(s)` base outright rather than letting a typo become a
    /// runtime error on every send, and rejects a blank key rather than sending
    /// an empty header the Worker would answer with 401.
    pub fn normalize(base: Option<String>, key: Option<String>) -> Option<Self> {
        let base = base.map(|s| s.trim().trim_end_matches('/').to_string())?;
        if base.is_empty() || !(base.starts_with("https://") || base.starts_with("http://")) {
            return None;
        }
        let write_key = key
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        Some(Self {
            ingest_url: format!("{base}/v1/apps/{APP_ID}/ingest"),
            base_url: base,
            write_key,
        })
    }

    /// The deletion URL for one retired install id.
    pub fn delete_url(&self, install_id: &str) -> String {
        format!("{}/v1/apps/{APP_ID}/install/{install_id}", self.base_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_halves_are_required() {
        assert_eq!(TelemetryEndpoint::normalize(None, None), None);
        assert_eq!(
            TelemetryEndpoint::normalize(Some("https://t.example".into()), None),
            None,
            "a URL without a key would post and be permanently rejected"
        );
        assert_eq!(TelemetryEndpoint::normalize(None, Some("k".into())), None);
        assert_eq!(
            TelemetryEndpoint::normalize(Some("https://t.example".into()), Some("  ".into())),
            None,
            "a blank key is a missing key, not an empty header"
        );
    }

    #[test]
    fn a_non_http_base_is_refused_at_construction() {
        // Better a build with no sender than one that fails on every send.
        for base in [
            "telemetry.sundaysuite.app",
            "ftp://t.example",
            "",
            "   ",
            "file:///etc/passwd",
        ] {
            assert_eq!(
                TelemetryEndpoint::normalize(Some(base.into()), Some("k".into())),
                None,
                "{base:?}"
            );
        }
    }

    #[test]
    fn urls_are_the_app_scoped_routes() {
        let c = TelemetryEndpoint::normalize(
            Some("https://telemetry.sundaysuite.app/".into()),
            Some(" key123 ".into()),
        )
        .expect("valid");
        assert_eq!(c.base_url, "https://telemetry.sundaysuite.app");
        assert_eq!(
            c.ingest_url, "https://telemetry.sundaysuite.app/v1/apps/sundaystage/ingest",
            "a trailing slash on the base must not produce a double slash"
        );
        assert_eq!(c.write_key, "key123");
        assert_eq!(
            c.delete_url("018f3a2b-7c4d-7e1f-9a2b-3c4d5e6f7a8b"),
            "https://telemetry.sundaysuite.app/v1/apps/sundaystage/install/018f3a2b-7c4d-7e1f-9a2b-3c4d5e6f7a8b"
        );
    }

    #[test]
    fn the_frozen_sundayrec_routes_are_never_produced_from_here() {
        // Law 4: Rec's live fleet owns `/v1/ingest` and `x-sundayrec-key`
        // forever. Stage must not be able to reach either by construction.
        let c = TelemetryEndpoint::normalize(
            Some("https://telemetry.sundaysuite.app".into()),
            Some("k".into()),
        )
        .expect("valid");
        assert!(c.ingest_url.contains("/v1/apps/sundaystage/"));
        assert!(!c.ingest_url.ends_with("/v1/ingest"));
        assert_eq!(WRITE_KEY_HEADER, "x-write-key");
        assert_ne!(WRITE_KEY_HEADER, "x-sundayrec-key");
    }

    #[test]
    fn a_localhost_base_is_allowed_for_wrangler_dev() {
        // E6/E7's live verification runs against `npx wrangler dev` over plain
        // http.
        let c = TelemetryEndpoint::normalize(
            Some("http://127.0.0.1:8787".into()),
            Some("dev-key".into()),
        )
        .expect("valid");
        assert_eq!(
            c.ingest_url,
            "http://127.0.0.1:8787/v1/apps/sundaystage/ingest"
        );
    }

    #[test]
    fn this_build_has_no_endpoint() {
        // The state every dev build and every CI run is in, asserted rather than
        // assumed: with no baked-in variables there is nothing to construct a
        // sender from, so nothing in this test suite can reach a network.
        if option_env!("SUNDAYSTAGE_TELEMETRY_URL").is_none()
            || option_env!("SUNDAYSTAGE_TELEMETRY_WRITE_KEY").is_none()
        {
            assert_eq!(TelemetryEndpoint::resolve(), None);
        }
    }
}
