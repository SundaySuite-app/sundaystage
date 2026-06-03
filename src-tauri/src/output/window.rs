//! Phase 5.2 — Tauri window glue for the live output.
//!
//! Opens one borderless full-screen webview window per assigned monitor, each
//! loading the dedicated `output.html` renderer. The render/heartbeat signal
//! flows over the Tauri event bus, emitted by the operator UI (see
//! `src/lib/outputBridge.ts`) — so if the operator UI freezes, the heartbeat
//! stops and each output window's watchdog holds the last frame instead of
//! blanking the congregation.
//!
//! This needs a real windowing session, so it is compiled but not unit-tested;
//! the role-resolution logic it relies on lives in [`crate::services::display`]
//! and is fully tested there.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::services::display::{DisplayAssignment, DisplayRole, MonitorInfo};

const LABEL_PREFIX: &str = "output-";

/// The window-label slug for a role. `None` for [`DisplayRole::Off`] — those
/// screens never get an output window. The slug is part of the window label,
/// which the renderer parses to pick its chrome (`OutputView.tsx`'s
/// `roleFromLabel`), so these strings are a wire contract: `-main-`/`-stage-`/
/// `-confidence-` must stay in sync with the frontend.
fn role_slug(role: DisplayRole) -> Option<&'static str> {
    match role {
        DisplayRole::MainOutput => Some("main"),
        DisplayRole::StageDisplay => Some("stage"),
        DisplayRole::ConfidenceMonitor => Some("confidence"),
        DisplayRole::Off => None,
    }
}

/// Build the unique window label for an output on `monitor_index` with `role`.
/// Returns `None` for `Off`. Pure (no `AppHandle`) so the label contract the
/// renderer parses is unit-testable. Shape: `output-<slug>-<index>`.
fn output_label(role: DisplayRole, monitor_index: u32) -> Option<String> {
    role_slug(role).map(|slug| format!("{LABEL_PREFIX}{slug}-{monitor_index}"))
}

/// Enumerate the OS monitors as our pure [`MonitorInfo`] model. The "primary"
/// screen is matched by position against the OS primary monitor.
pub fn list_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary_pos = app.primary_monitor().ok().flatten().map(|m| {
        let p = m.position();
        (p.x, p.y)
    });

    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let pos = m.position();
            let size = m.size();
            MonitorInfo {
                index: i as u32,
                name: m
                    .name()
                    .cloned()
                    .unwrap_or_else(|| format!("Skjerm {}", i + 1)),
                width: size.width,
                height: size.height,
                x: pos.x,
                y: pos.y,
                scale_factor: m.scale_factor(),
                is_primary: Some((pos.x, pos.y)) == primary_pos,
            }
        })
        .collect()
}

/// Open one full-screen output window per non-off assignment, replacing any
/// already open. The window is placed on the target monitor, then fullscreened.
pub fn open_outputs(
    app: &AppHandle,
    monitors: &[MonitorInfo],
    assignments: &[DisplayAssignment],
) -> Result<(), String> {
    close_outputs(app);
    for a in assignments {
        let Some(label) = output_label(a.role, a.monitor_index) else {
            continue;
        };
        let Some(m) = monitors.iter().find(|m| m.index == a.monitor_index) else {
            continue;
        };
        let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("output.html".into()))
            .title("SundayStage")
            // Borderless: no titlebar/chrome over the congregation slide.
            .decorations(false)
            // Place the window on the target monitor *before* fullscreening —
            // `set_fullscreen` adopts the monitor the window currently sits on,
            // so positioning first is what pins each output to its own screen.
            .position(m.x as f64, m.y as f64)
            .inner_size(m.width as f64, m.height as f64)
            // Sit above other apps so a stray window can never cover the slide,
            // and stay out of the taskbar/dock — these are appliance surfaces,
            // not windows the operator alt-tabs through.
            .always_on_top(true)
            .skip_taskbar(true)
            // Never steal keyboard focus from the operator console — the
            // operator's cue-advance hotkeys are bound there, and a focused
            // output window would silently swallow them mid-service.
            .focused(false)
            .build()
            .map_err(|e| e.to_string())?;
        // Borderless full-screen on the monitor the window now sits on.
        let _ = win.set_fullscreen(true);
    }
    Ok(())
}

/// Close every output window (leaves the operator window untouched).
pub fn close_outputs(app: &AppHandle) {
    for (label, win) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let _ = win.close();
        }
    }
}

/// Are any output windows currently open?
pub fn outputs_open(app: &AppHandle) -> bool {
    app.webview_windows()
        .keys()
        .any(|l| l.starts_with(LABEL_PREFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The window functions need a real `AppHandle` (a windowing session), so
    // only the pure label contract is unit-tested here. That label is the wire
    // contract the renderer parses (`OutputView.tsx` `roleFromLabel`) and the
    // key `close_outputs`/`outputs_open` match on, so guarding its shape is the
    // meaningful headless test.

    #[test]
    fn label_carries_role_slug_and_monitor_index() {
        assert_eq!(
            output_label(DisplayRole::MainOutput, 0).as_deref(),
            Some("output-main-0")
        );
        assert_eq!(
            output_label(DisplayRole::StageDisplay, 1).as_deref(),
            Some("output-stage-1")
        );
        assert_eq!(
            output_label(DisplayRole::ConfidenceMonitor, 2).as_deref(),
            Some("output-confidence-2")
        );
    }

    #[test]
    fn off_role_gets_no_window_label() {
        assert_eq!(output_label(DisplayRole::Off, 0), None);
        assert_eq!(role_slug(DisplayRole::Off), None);
    }

    #[test]
    fn every_label_starts_with_the_prefix_we_match_on() {
        // close_outputs / outputs_open identify output windows by this prefix —
        // if a label ever stopped carrying it they would leak or report wrong.
        for role in [
            DisplayRole::MainOutput,
            DisplayRole::StageDisplay,
            DisplayRole::ConfidenceMonitor,
        ] {
            let label = output_label(role, 3).expect("driven role has a label");
            assert!(label.starts_with(LABEL_PREFIX), "label was {label}");
        }
    }

    #[test]
    fn label_slugs_match_the_renderer_role_substrings() {
        // OutputView.tsx's roleFromLabel keys off these exact substrings to pick
        // stage/confidence chrome; main is the fallback. Keep them in lockstep.
        assert!(output_label(DisplayRole::StageDisplay, 7)
            .unwrap()
            .contains("-stage-"));
        assert!(output_label(DisplayRole::ConfidenceMonitor, 7)
            .unwrap()
            .contains("-confidence-"));
        // Main carries neither stage nor confidence → renderer falls back to main.
        let main = output_label(DisplayRole::MainOutput, 7).unwrap();
        assert!(!main.contains("-stage-") && !main.contains("-confidence-"));
    }

    #[test]
    fn labels_are_unique_per_role_and_monitor() {
        let a = output_label(DisplayRole::MainOutput, 0).unwrap();
        let b = output_label(DisplayRole::StageDisplay, 0).unwrap();
        let c = output_label(DisplayRole::MainOutput, 1).unwrap();
        assert_ne!(a, b); // same monitor, different role
        assert_ne!(a, c); // same role, different monitor
    }
}
