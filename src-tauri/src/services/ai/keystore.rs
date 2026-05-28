//! Phase 4.1 — API-key storage in the OS keychain.
//!
//! The Anthropic key is sensitive: per the plan it lives in the system keychain
//! (Keychain Access on macOS, Credential Manager on Windows, Secret Service on
//! Linux) via the `keyring` crate — **never** in a plaintext config file and
//! never bundled with the app. The renderer sets/clears it through the
//! `ai_key_*` commands; AI commands resolve a key in priority order:
//! explicit (a one-off pasted key) → keychain → `ANTHROPIC_API_KEY` env.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "no.sundaystage.app";
const ACCOUNT: &str = "anthropic_api_key";

fn entry() -> AppResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| AppError::Internal(format!("nøkkellager: {e}")))
}

/// Store the API key in the OS keychain.
pub fn set_key(key: &str) -> AppResult<()> {
    entry()?
        .set_password(key)
        .map_err(|e| AppError::Internal(format!("kunne ikke lagre nøkkel: {e}")))
}

/// Remove the stored key (no error if there was none).
pub fn clear_key() -> AppResult<()> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Internal(format!("kunne ikke slette nøkkel: {e}"))),
    }
}

/// The stored key, or `None` if absent / the keychain is unavailable.
pub fn get_key() -> Option<String> {
    entry().ok()?.get_password().ok()
}

/// Whether a key is stored in the keychain.
pub fn has_key() -> bool {
    get_key().is_some()
}

/// Whether the `ANTHROPIC_API_KEY` env var is set (non-empty).
pub fn has_env_key() -> bool {
    std::env::var("ANTHROPIC_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// Resolve the key to use: an explicit (non-empty) one wins, then the keychain,
/// then the environment.
pub fn resolve(explicit: Option<String>) -> Option<String> {
    explicit
        .filter(|k| !k.trim().is_empty())
        .or_else(get_key)
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .filter(|k| !k.trim().is_empty())
}
