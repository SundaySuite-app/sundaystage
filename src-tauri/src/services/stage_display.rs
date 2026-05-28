//! Phase 8 — stage display configuration.
//!
//! The stage display is the screen the worship leader and musicians watch:
//! current/next slide, the section label (verse/chorus — musicians steer by
//! it), a clock + service timer, and notes. Different people want different
//! panels, so a stage display is a [`StageDisplayConfig`] of panel toggles,
//! saved as a preset and (eventually) assigned to a physical screen.
//!
//! Only the configuration lives in Rust — it's the part worth a stable model +
//! tests. The actual rendering happens in the frontend (and, once Phase 5.2's
//! output process exists, in a separate stage-display window with the same
//! crash isolation as the main output). The drag-and-drop layout builder and
//! per-screen assignment are deferred with that windowing work.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which panels a stage display shows. Presets are just named configs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/StageDisplayConfig.ts")]
pub struct StageDisplayConfig {
    pub id: String,
    pub name: String,
    /// Large preview of what's on the main output now.
    pub show_current_slide: bool,
    /// Smaller preview of the next cue.
    pub show_next_slide: bool,
    /// Render lyrics as big plain text instead of a slide preview.
    pub lyrics_large: bool,
    /// "Verse 1" / "Chorus" — musicians need this.
    pub show_section_label: bool,
    /// Wall clock.
    pub show_clock: bool,
    /// Count-up since the service went live.
    pub show_service_timer: bool,
    /// Notes / sermon outline / chord chart.
    pub show_notes: bool,
}

/// Built-in presets covering the common roles.
pub fn builtin_stage_presets() -> Vec<StageDisplayConfig> {
    vec![
        StageDisplayConfig {
            id: "stage-worship-leader".into(),
            name: "Lovsangsleder".into(),
            show_current_slide: true,
            show_next_slide: true,
            lyrics_large: true,
            show_section_label: true,
            show_clock: true,
            show_service_timer: true,
            show_notes: true,
        },
        StageDisplayConfig {
            id: "stage-musician".into(),
            name: "Musiker".into(),
            show_current_slide: true,
            show_next_slide: true,
            lyrics_large: true,
            show_section_label: true,
            show_clock: false,
            show_service_timer: false,
            show_notes: false,
        },
        StageDisplayConfig {
            id: "stage-pastor".into(),
            name: "Forkynner".into(),
            show_current_slide: false,
            show_next_slide: false,
            lyrics_large: false,
            show_section_label: false,
            show_clock: true,
            show_service_timer: true,
            show_notes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_builtin_presets_with_unique_ids() {
        let presets = builtin_stage_presets();
        assert_eq!(presets.len(), 3);
        let mut ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n, "preset ids must be unique");
    }

    #[test]
    fn musician_preset_shows_section_label_but_hides_clock() {
        let m = builtin_stage_presets()
            .into_iter()
            .find(|p| p.id == "stage-musician")
            .unwrap();
        assert!(m.show_section_label, "musicians steer by the section label");
        assert!(!m.show_service_timer, "musicians don't need the service timer");
    }

    #[test]
    fn pastor_preset_centres_on_notes_and_clock() {
        let p = builtin_stage_presets()
            .into_iter()
            .find(|p| p.id == "stage-pastor")
            .unwrap();
        assert!(p.show_notes);
        assert!(p.show_clock);
        assert!(!p.show_current_slide);
    }
}
