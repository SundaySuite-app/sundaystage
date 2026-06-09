//! Sunday Account (SSO) commands — read-only, network-free.
//!
//! `sunday_account_status` reads the shared session this and every Sunday app
//! share, so a login performed in SundayRec is visible here ("log in once → all
//! apps"). `sunday_sign_out` clears that shared session. The browser login flow
//! itself lives in SundayRec; this local-first app only reads.

use crate::account::{self, AccountStatus};

/// The current Sunday Account status from the shared session (signed-out if none).
#[tauri::command]
pub fn sunday_account_status() -> Result<AccountStatus, String> {
    account::status()
}

/// Local sign-out across the whole suite (clears the shared session file).
#[tauri::command]
pub fn sunday_sign_out() -> Result<(), String> {
    account::sign_out()
}
