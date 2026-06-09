//! Sunday Account (SSO) — read-only view of the shared session.
//!
//! This is a local-first app: it never logs in itself and runs fully offline.
//! The browser login lives in SundayRec; here we only READ the shared session
//! file (`<app-data>/SundaySuite/session.json`) that any Sunday app writes, so
//! once the user has signed in anywhere this app shows "signed in as X" and can
//! gate its cloud / cross-app UI on the cached claims. No network, no secrets —
//! a missing file simply means signed-out, and auth never gates local features.
//!
//! The login flow + token refresh (the impure loopback/reqwest half) live in
//! SundayRec's `account.rs`; this app gains them only if/when it grows a real
//! cloud feature. Until then, reading is all it needs. Errors are plain strings
//! (these commands are trivial reads) so this module drops into every local app
//! unchanged, regardless of that app's own error enum.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sunday_auth::session;

/// The account state the renderer renders. `signed_in=false` means no shared
/// session is present; all other fields are then defaults.
#[derive(Debug, Clone, Serialize)]
pub struct AccountStatus {
    pub signed_in: bool,
    pub sub: String,
    pub email: Option<String>,
    #[serde(rename = "churchIds")]
    pub church_ids: Vec<String>,
    #[serde(rename = "appGrants")]
    pub app_grants: std::collections::BTreeMap<String, Vec<String>>,
    /// Whether the cached claims are still within their offline-grace window.
    #[serde(rename = "claimsFresh")]
    pub claims_fresh: bool,
}

impl AccountStatus {
    /// The signed-out state (no shared session present).
    pub fn signed_out() -> Self {
        Self {
            signed_in: false,
            sub: String::new(),
            email: None,
            church_ids: Vec::new(),
            app_grants: std::collections::BTreeMap::new(),
            claims_fresh: false,
        }
    }

    /// Project a stored session into the renderer-facing status. Pure.
    pub fn from_session(data: &session::SessionData, now_ms: i64) -> Self {
        let c = &data.cached_claims;
        Self {
            signed_in: true,
            sub: c.sub.clone(),
            email: c.email.clone(),
            church_ids: c.church_ids.clone(),
            app_grants: c.app_grants.clone(),
            claims_fresh: data.claims_fresh(now_ms),
        }
    }
}

/// Unix milliseconds (for the offline-grace check).
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Read the current account status from the shared session (signed-out if none).
pub fn status() -> Result<AccountStatus, String> {
    let path = session::default_path().ok_or("kunne ikke finne delt sesjons-katalog")?;
    match session::read(&path).map_err(|e| e.to_string())? {
        Some(data) => Ok(AccountStatus::from_session(&data, now_ms())),
        None => Ok(AccountStatus::signed_out()),
    }
}

/// Local sign-out: clear the shared session so every Sunday app is logged out.
pub fn sign_out() -> Result<(), String> {
    let path = session::default_path().ok_or("kunne ikke finne delt sesjons-katalog")?;
    session::clear(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_out_status_is_empty() {
        let s = AccountStatus::signed_out();
        assert!(!s.signed_in);
        assert!(s.church_ids.is_empty());
        assert!(!s.claims_fresh);
    }

    #[test]
    fn from_session_projects_claims_and_freshness() {
        let mut grants = std::collections::BTreeMap::new();
        grants.insert("c1".to_string(), vec!["stage".to_string()]);
        let data = session::SessionData {
            schema_version: session::SESSION_SCHEMA_VERSION,
            refresh_token: "RT".into(),
            cached_claims: session::SundayClaims {
                sub: "u1".into(),
                church_ids: vec!["c1".into()],
                app_grants: grants,
                email: Some("a@b.no".into()),
            },
            claims_expires_at_ms: 1_000,
            issuer: "https://auth.sundaysuite.app/auth/v1".into(),
        };
        let fresh = AccountStatus::from_session(&data, 999);
        assert!(fresh.signed_in);
        assert_eq!(fresh.church_ids, vec!["c1".to_string()]);
        assert!(fresh.claims_fresh);
        assert!(!AccountStatus::from_session(&data, 2_000).claims_fresh);
    }
}
