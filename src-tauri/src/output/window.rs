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

fn role_slug(role: DisplayRole) -> Option<&'static str> {
    match role {
        DisplayRole::MainOutput => Some("main"),
        DisplayRole::StageDisplay => Some("stage"),
        DisplayRole::ConfidenceMonitor => Some("confidence"),
        DisplayRole::Off => None,
    }
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
        let Some(slug) = role_slug(a.role) else {
            continue;
        };
        let Some(m) = monitors.iter().find(|m| m.index == a.monitor_index) else {
            continue;
        };
        let label = format!("{LABEL_PREFIX}{slug}-{}", a.monitor_index);
        let win = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("output.html".into()))
            .title("SundayStage")
            .decorations(false)
            .position(m.x as f64, m.y as f64)
            .inner_size(m.width as f64, m.height as f64)
            .build()
            .map_err(|e| e.to_string())?;
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
