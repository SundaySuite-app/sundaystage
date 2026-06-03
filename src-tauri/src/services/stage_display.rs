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

/// The output role a service template targets — *who* watches the stage
/// display when the template runs. Templates carry this so the operator can see,
/// per template, which panel layout the screen will surface. The frontend
/// persists the per-template assignment (per device); this enum + mapping is the
/// shared source of truth for the panel layout each role gets, mirrored in
/// `src/features/services/templateRoles.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings/TemplateRole.ts")]
#[serde(rename_all = "kebab-case")]
pub enum TemplateRole {
    WorshipLeader,
    Musician,
    Operator,
    Congregation,
}

impl TemplateRole {
    /// Panel layout this role's stage display shows. The `name`/`id` fields are
    /// placeholders — only the panel toggles are role-meaningful here.
    pub fn stage_config(self) -> StageDisplayConfig {
        match self {
            // Worship leader steers the set: everything visible.
            TemplateRole::WorshipLeader => StageDisplayConfig {
                id: "role-worship-leader".into(),
                name: "Lovsangsleder".into(),
                show_current_slide: true,
                show_next_slide: true,
                lyrics_large: true,
                show_section_label: true,
                show_clock: true,
                show_service_timer: true,
                show_notes: true,
            },
            // Musician: lyrics + section label, no clock/timer/notes clutter.
            TemplateRole::Musician => StageDisplayConfig {
                id: "role-musician".into(),
                name: "Musiker".into(),
                show_current_slide: true,
                show_next_slide: true,
                lyrics_large: true,
                show_section_label: true,
                show_clock: false,
                show_service_timer: false,
                show_notes: false,
            },
            // Operator: cue-list confidence — current + next + timing + notes.
            TemplateRole::Operator => StageDisplayConfig {
                id: "role-operator".into(),
                name: "Operatør".into(),
                show_current_slide: true,
                show_next_slide: true,
                lyrics_large: false,
                show_section_label: true,
                show_clock: true,
                show_service_timer: true,
                show_notes: true,
            },
            // Congregation: just the slide on screen, nothing else.
            TemplateRole::Congregation => StageDisplayConfig {
                id: "role-congregation".into(),
                name: "Menighet".into(),
                show_current_slide: true,
                show_next_slide: false,
                lyrics_large: true,
                show_section_label: false,
                show_clock: false,
                show_service_timer: false,
                show_notes: false,
            },
        }
    }
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
        assert!(
            !m.show_service_timer,
            "musicians don't need the service timer"
        );
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

    #[test]
    fn template_role_serialises_kebab_case() {
        // Must match the frontend `TemplateRole` string union exactly.
        let json = serde_json::to_string(&TemplateRole::WorshipLeader).unwrap();
        assert_eq!(json, "\"worship-leader\"");
        let back: TemplateRole = serde_json::from_str("\"congregation\"").unwrap();
        assert_eq!(back, TemplateRole::Congregation);
    }

    #[test]
    fn musician_role_hides_clock_keeps_section_label() {
        let c = TemplateRole::Musician.stage_config();
        assert!(c.show_section_label);
        assert!(!c.show_clock);
        assert!(!c.show_service_timer);
        assert!(!c.show_notes);
    }

    #[test]
    fn congregation_role_shows_only_current_slide() {
        let c = TemplateRole::Congregation.stage_config();
        assert!(c.show_current_slide);
        assert!(!c.show_next_slide);
        assert!(!c.show_section_label);
        assert!(!c.show_clock);
        assert!(!c.show_notes);
    }

    #[test]
    fn each_role_has_a_distinct_panel_layout() {
        use TemplateRole::*;
        let roles = [WorshipLeader, Musician, Operator, Congregation];
        let mut sigs: Vec<_> = roles
            .iter()
            .map(|r| {
                let c = r.stage_config();
                (
                    c.show_current_slide,
                    c.show_next_slide,
                    c.lyrics_large,
                    c.show_section_label,
                    c.show_clock,
                    c.show_service_timer,
                    c.show_notes,
                )
            })
            .collect();
        let n = sigs.len();
        sigs.sort();
        sigs.dedup();
        assert_eq!(sigs.len(), n, "each role must map to a distinct layout");
    }
}
