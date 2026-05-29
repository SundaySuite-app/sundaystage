//! Phase 5.2 — display detection + role assignment (pure core).
//!
//! The operator's machine may drive several screens: the projector/TV the
//! congregation sees, a stage-facing confidence monitor for musicians, and the
//! laptop panel the operator works on. This module owns the *pure* part of that
//! story — modelling monitors and resolving which screen plays which role — so
//! it is fully unit-testable. The Tauri glue that actually enumerates monitors
//! and opens borderless full-screen windows lives in `output::window` (compiled,
//! but only exercisable in a real windowing session).
//!
//! Reliability principle: we never want the congregation to lose the slide, so
//! when a screen is unplugged mid-service [`reconcile`] keeps a main output
//! alive on any remaining *external* screen — but it never forces full-screen
//! output onto the operator's own (primary) screen.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::services::slide_doc::HAlign;

/// How the congregation output renders slides — independent of *which* screen
/// shows them (that's [`OutputConfig`]). Persisted per machine; the output
/// windows read it on open and update live when it changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/OutputAppearance.ts")]
pub struct OutputAppearance {
    /// Multiplier on the base slide font size. 1.0 = default.
    pub text_scale: f32,
    /// Lyric/text colour (hex, e.g. "#ffffff").
    pub text_color: String,
    /// Background colour (hex).
    pub bg_color: String,
    /// Horizontal alignment of the lyric block.
    pub h_align: HAlign,
    /// Show the section label ("Verse 1") on the congregation output. Many
    /// churches want this off for the audience and on for the stage.
    pub show_section_label: bool,
    /// Render lyric lines in UPPERCASE.
    pub uppercase: bool,
    /// Line-height multiplier.
    pub line_height: f32,
}

impl Default for OutputAppearance {
    fn default() -> Self {
        Self {
            text_scale: 1.0,
            text_color: "#ffffff".into(),
            bg_color: "#0a1730".into(),
            h_align: HAlign::Center,
            show_section_label: true,
            uppercase: false,
            line_height: 1.1,
        }
    }
}

impl OutputAppearance {
    /// Clamp numeric fields to sane ranges so a hand-edited config (or a buggy
    /// slider) can never make the output unreadable.
    pub fn sanitized(mut self) -> Self {
        self.text_scale = self.text_scale.clamp(0.5, 2.5);
        self.line_height = self.line_height.clamp(0.9, 2.5);
        if self.text_color.trim().is_empty() {
            self.text_color = "#ffffff".into();
        }
        if self.bg_color.trim().is_empty() {
            self.bg_color = "#0a1730".into();
        }
        self
    }
}

/// What a given physical screen is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/lib/bindings/DisplayRole.ts")]
pub enum DisplayRole {
    /// The congregation-facing output (projector/TV). Clean slides only.
    MainOutput,
    /// Stage-facing monitor for the worship leader/musicians (extra chrome:
    /// next line, clock, section label, lost-connection badge).
    StageDisplay,
    /// A confidence monitor — same as main output but on a screen only the
    /// platform party sees.
    ConfidenceMonitor,
    /// Not driven by SundayStage (e.g. the operator's own laptop screen).
    Off,
}

/// A connected monitor as reported by the OS. `index` is the position in the
/// enumeration and is the stable-enough key we assign roles against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/MonitorInfo.ts")]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    /// The OS "primary" screen — assumed to be where the operator UI lives.
    pub is_primary: bool,
}

/// Binds a monitor to a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/DisplayAssignment.ts")]
pub struct DisplayAssignment {
    pub monitor_index: u32,
    pub role: DisplayRole,
}

/// Persisted output configuration (the user's last-chosen role assignments).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/OutputConfig.ts")]
pub struct OutputConfig {
    pub assignments: Vec<DisplayAssignment>,
}

/// First sensible setup for a fresh machine: the first *external* screen becomes
/// the main output; everything else (including the operator's primary) is off.
/// With a single screen, nothing is driven — the operator uses the in-app
/// preview.
pub fn default_assignments(monitors: &[MonitorInfo]) -> Vec<DisplayAssignment> {
    let first_external = monitors.iter().find(|m| !m.is_primary).map(|m| m.index);
    monitors
        .iter()
        .map(|m| DisplayAssignment {
            monitor_index: m.index,
            role: if Some(m.index) == first_external {
                DisplayRole::MainOutput
            } else {
                DisplayRole::Off
            },
        })
        .collect()
}

/// Reconcile saved assignments against the monitors currently present (hot-swap
/// when a screen is plugged/unplugged mid-session):
///   - keep each surviving monitor's saved role; new monitors arrive as `Off`;
///   - guarantee at most one `MainOutput`;
///   - if no monitor carries `MainOutput` but an external screen exists, promote
///     the first external one (so the congregation never goes dark) — but never
///     hijack the operator's primary screen.
pub fn reconcile(saved: &[DisplayAssignment], monitors: &[MonitorInfo]) -> Vec<DisplayAssignment> {
    let mut out: Vec<DisplayAssignment> = monitors
        .iter()
        .map(|m| {
            let role = saved
                .iter()
                .find(|a| a.monitor_index == m.index)
                .map(|a| a.role)
                .unwrap_or(DisplayRole::Off);
            DisplayAssignment {
                monitor_index: m.index,
                role,
            }
        })
        .collect();

    // Collapse duplicate main outputs: keep the first, demote the rest.
    let mut seen_main = false;
    for a in out.iter_mut() {
        if a.role == DisplayRole::MainOutput {
            if seen_main {
                a.role = DisplayRole::Off;
            } else {
                seen_main = true;
            }
        }
    }

    // If nothing drives the main output, promote the first external screen.
    if !seen_main {
        if let Some(target) = monitors.iter().find(|m| !m.is_primary).map(|m| m.index) {
            if let Some(a) = out.iter_mut().find(|a| a.monitor_index == target) {
                a.role = DisplayRole::MainOutput;
            }
        }
    }

    out
}

/// The monitor indices that should host an output window (everything but `Off`).
pub fn active_indices(assignments: &[DisplayAssignment]) -> Vec<u32> {
    assignments
        .iter()
        .filter(|a| a.role != DisplayRole::Off)
        .map(|a| a.monitor_index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(index: u32, primary: bool) -> MonitorInfo {
        MonitorInfo {
            index,
            name: format!("Monitor {index}"),
            width: 1920,
            height: 1080,
            x: (index as i32) * 1920,
            y: 0,
            scale_factor: 1.0,
            is_primary: primary,
        }
    }

    #[test]
    fn single_monitor_drives_nothing() {
        let a = default_assignments(&[mon(0, true)]);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].role, DisplayRole::Off);
        assert!(active_indices(&a).is_empty());
    }

    #[test]
    fn external_monitor_becomes_main_output() {
        let a = default_assignments(&[mon(0, true), mon(1, false)]);
        assert_eq!(a[0].role, DisplayRole::Off); // operator's primary
        assert_eq!(a[1].role, DisplayRole::MainOutput); // projector
        assert_eq!(active_indices(&a), vec![1]);
    }

    #[test]
    fn first_external_wins_with_several() {
        let a = default_assignments(&[mon(0, true), mon(1, false), mon(2, false)]);
        assert_eq!(a[1].role, DisplayRole::MainOutput);
        assert_eq!(a[2].role, DisplayRole::Off);
    }

    #[test]
    fn reconcile_preserves_user_roles() {
        let saved = vec![
            DisplayAssignment {
                monitor_index: 1,
                role: DisplayRole::MainOutput,
            },
            DisplayAssignment {
                monitor_index: 2,
                role: DisplayRole::StageDisplay,
            },
        ];
        let got = reconcile(&saved, &[mon(0, true), mon(1, false), mon(2, false)]);
        assert_eq!(got[0].role, DisplayRole::Off);
        assert_eq!(got[1].role, DisplayRole::MainOutput);
        assert_eq!(got[2].role, DisplayRole::StageDisplay);
    }

    #[test]
    fn reconcile_promotes_when_main_output_unplugged() {
        // Saved had the projector on index 2; now only the primary (0) and one
        // external (1) remain — index 1 should be promoted to keep output alive.
        let saved = vec![DisplayAssignment {
            monitor_index: 2,
            role: DisplayRole::MainOutput,
        }];
        let got = reconcile(&saved, &[mon(0, true), mon(1, false)]);
        assert_eq!(got[1].role, DisplayRole::MainOutput);
    }

    #[test]
    fn reconcile_never_hijacks_the_only_screen() {
        // Down to just the operator's laptop — do not force full-screen output
        // over the operator.
        let saved = vec![DisplayAssignment {
            monitor_index: 1,
            role: DisplayRole::MainOutput,
        }];
        let got = reconcile(&saved, &[mon(0, true)]);
        assert_eq!(got[0].role, DisplayRole::Off);
        assert!(active_indices(&got).is_empty());
    }

    #[test]
    fn reconcile_collapses_duplicate_main_outputs() {
        let saved = vec![
            DisplayAssignment {
                monitor_index: 1,
                role: DisplayRole::MainOutput,
            },
            DisplayAssignment {
                monitor_index: 2,
                role: DisplayRole::MainOutput,
            },
        ];
        let got = reconcile(&saved, &[mon(0, true), mon(1, false), mon(2, false)]);
        assert_eq!(got[1].role, DisplayRole::MainOutput);
        assert_eq!(got[2].role, DisplayRole::Off);
    }

    #[test]
    fn config_round_trips() {
        let cfg = OutputConfig {
            assignments: default_assignments(&[mon(0, true), mon(1, false)]),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: OutputConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn appearance_default_and_clamping() {
        let d = OutputAppearance::default();
        assert_eq!(d.text_scale, 1.0);
        assert_eq!(d.h_align, HAlign::Center);
        assert!(d.show_section_label);

        let wild = OutputAppearance {
            text_scale: 9.0,
            line_height: 0.1,
            text_color: "".into(),
            bg_color: "   ".into(),
            ..OutputAppearance::default()
        }
        .sanitized();
        assert_eq!(wild.text_scale, 2.5);
        assert_eq!(wild.line_height, 0.9);
        assert_eq!(wild.text_color, "#ffffff");
        assert_eq!(wild.bg_color, "#0a1730");
    }

    #[test]
    fn appearance_round_trips() {
        let a = OutputAppearance::default();
        let json = serde_json::to_string(&a).unwrap();
        let back: OutputAppearance = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
