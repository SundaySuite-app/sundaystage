//! Crash-reporting commands, on the E3 ring.
//!
//! Local capture is ALWAYS ON since E5, so the `status` / `set` pair is gone:
//! there is no longer a local setting to read or write. The ring never leaves
//! the machine, which is exactly why it needs no permission — and the old
//! default (off) meant the records did not exist on the machines that most
//! needed them. See [`crate::telemetry::crash_ring::RETIRED_FLAG_FILE`].
//!
//! What DOES need permission is transmission, and that is the separate
//! three-state consent record in [`crate::telemetry::consent`], reached through
//! the `telemetry.*` commands. Retiring the local flag grants none of it.
//!
//! [`crash_report_frontend`] is the renderer's route into the same ring. A
//! `window.onerror` in the operator workspace is a crash the operator
//! experienced; it belongs in the same file the panic hook writes to, through
//! the same scrubbing and the same caps.

use tauri::State;

use crate::error::AppResult;
use crate::telemetry::crash_ring::{self, CrashKind};
use crate::AppState;

/// How many crash records the ring holds.
#[tauri::command]
pub fn crash_reports_count(state: State<'_, AppState>) -> usize {
    crash_ring::count(&crash_ring::crash_dir(&state.data_dir))
}

/// Delete every crash record.
#[tauri::command]
pub fn crash_reports_clear(state: State<'_, AppState>) -> AppResult<()> {
    crash_ring::clear(&crash_ring::crash_dir(&state.data_dir))?;
    Ok(())
}

/// Record an error the RENDERER caught: `window.onerror`, an unhandled promise
/// rejection, or a React error boundary.
///
/// `kind` is a [`CrashKind`], so serde rejects anything outside the closed set
/// before this body runs — the IPC boundary cannot widen the vocabulary. The
/// message and location go through the same scrubbing and the same caps as a
/// Rust panic, so a stack frame naming `/Users/ola/…` never reaches disk.
///
/// Returns `()` and never errors: a reporting path that can fail is a reporting
/// path the renderer has to handle, and the renderer's error handler is the
/// last place that should have error handling of its own.
#[tauri::command]
pub fn crash_report_frontend(
    state: State<'_, AppState>,
    kind: CrashKind,
    message: String,
    location: Option<String>,
    component: Option<String>,
) {
    // The renderer cannot reach the ring except through `crash_ring::record`,
    // which applies the scrubbing and the caps itself.
    let _ = &state;
    crash_ring::record(kind, &message, location.as_deref(), component.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::crash_ring::{crash_dir, read_entries};

    #[test]
    fn the_frontend_kinds_are_the_only_ones_the_boundary_accepts() {
        // The renderer sends a JSON string; serde is the allow-list.
        for wire in ["webview_error", "unhandled_rejection", "other"] {
            let parsed: CrashKind =
                serde_json::from_str(&format!("\"{wire}\"")).expect("a known kind");
            assert_eq!(
                serde_json::to_string(&parsed).expect("round trip"),
                format!("\"{wire}\"")
            );
        }
        assert!(serde_json::from_str::<CrashKind>("\"lyrics\"").is_err());
        assert!(serde_json::from_str::<CrashKind>("\"WebviewError\"").is_err());
    }

    #[test]
    fn a_frontend_report_lands_scrubbed_and_capped() {
        // The command's exact path, with the exact inputs a renderer sends: a
        // `TypeError` whose stack frame is full of the operator's home dir.
        let dir = tempfile::tempdir().expect("tempdir");
        let ring = crash_dir(dir.path());

        // No opt-in step: capture is always on since E5, because the ring is
        // the boundary — these files never leave the machine.
        crash_ring::record_in(
            dir.path(),
            CrashKind::UnhandledRejection,
            "TypeError: cannot read properties of undefined at /Users/ola/app/main.js:1:2",
            Some("file:///Users/ola/app/index.html:42:7"),
            Some("OperatorWorkspace"),
        );

        let back = read_entries(&ring);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].kind, CrashKind::UnhandledRejection);
        // The operator's home directory did not reach disk…
        assert!(
            !back[0].message.contains("/Users/"),
            "{:?}",
            back[0].message
        );
        // …but the error's identity — the only thing that tells two crashes
        // apart — did.
        assert!(back[0].message.starts_with("TypeError:"));
        assert_eq!(back[0].task.as_deref(), Some("OperatorWorkspace"));
        let loc = back[0].location.clone().expect("location");
        assert!(!loc.contains("/Users/"), "{loc}");
    }
}
