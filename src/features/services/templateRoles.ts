/**
 * Phase 8 — per-template stage-display role assignment.
 *
 * Every service template can be tagged with the *output role* it is built for:
 * who is meant to watch the stage display when this template runs. The role
 * determines which panels the stage screen surfaces (lyrics, next slide,
 * section label, clock, notes) — the same panel model the Rust
 * `StageDisplayConfig` presets use (see src-tauri/src/services/stage_display.rs).
 *
 * The assignment is a per-device UI preference, not synced data, so it lives in
 * localStorage keyed by template id — mirroring `aiConsent.ts`. The mapping
 * from role → panel preview is a pure function so the inspector's live preview
 * (and these tests) need no backend.
 */

export type TemplateRole =
  | "worship-leader"
  | "musician"
  | "operator"
  | "congregation";

export const TEMPLATE_ROLES: TemplateRole[] = [
  "worship-leader",
  "musician",
  "operator",
  "congregation",
];

/** The default role for a template with no explicit assignment. */
export const DEFAULT_ROLE: TemplateRole = "worship-leader";

/**
 * Which stage-display panels a role's screen shows. Matches the panel toggles
 * in the Rust `StageDisplayConfig`; the frontend `operator` and `congregation`
 * roles extend the three built-in stage presets.
 */
export interface RolePanels {
  showCurrentSlide: boolean;
  showNextSlide: boolean;
  lyricsLarge: boolean;
  showSectionLabel: boolean;
  showClock: boolean;
  showServiceTimer: boolean;
  showNotes: boolean;
}

const ROLE_PANELS: Record<TemplateRole, RolePanels> = {
  // Worship leader: sees everything to steer the set.
  "worship-leader": {
    showCurrentSlide: true,
    showNextSlide: true,
    lyricsLarge: true,
    showSectionLabel: true,
    showClock: true,
    showServiceTimer: true,
    showNotes: true,
  },
  // Musician: lyrics + section label, no clock/timer/notes clutter.
  musician: {
    showCurrentSlide: true,
    showNextSlide: true,
    lyricsLarge: true,
    showSectionLabel: true,
    showClock: false,
    showServiceTimer: false,
    showNotes: false,
  },
  // Operator: cue-list confidence — current + next + timing + notes.
  operator: {
    showCurrentSlide: true,
    showNextSlide: true,
    lyricsLarge: false,
    showSectionLabel: true,
    showClock: true,
    showServiceTimer: true,
    showNotes: true,
  },
  // Congregation: just the slide on screen, nothing else.
  congregation: {
    showCurrentSlide: true,
    showNextSlide: false,
    lyricsLarge: true,
    showSectionLabel: false,
    showClock: false,
    showServiceTimer: false,
    showNotes: false,
  },
};

/** Pure: the panel preview for a role. */
export function panelsForRole(role: TemplateRole): RolePanels {
  return ROLE_PANELS[role];
}

/** i18n key for a role's display name. */
export function roleLabelKey(role: TemplateRole): string {
  switch (role) {
    case "worship-leader":
      return "tmplRoleWorshipLeader";
    case "musician":
      return "tmplRoleMusician";
    case "operator":
      return "tmplRoleOperator";
    case "congregation":
      return "tmplRoleCongregation";
  }
}

// ── localStorage persistence (per device, keyed by template id) ────────────────

const ROLES_KEY = "ss-template-roles";

type RoleMap = Record<string, TemplateRole>;

function isRole(value: unknown): value is TemplateRole {
  return (
    typeof value === "string" && TEMPLATE_ROLES.includes(value as TemplateRole)
  );
}

/** Parse a stored role map, discarding anything unexpected. */
export function parseRoleMap(raw: string | null): RoleMap {
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};
    const out: RoleMap = {};
    for (const [id, role] of Object.entries(
      parsed as Record<string, unknown>,
    )) {
      if (isRole(role)) out[id] = role;
    }
    return out;
  } catch {
    return {};
  }
}

function loadRoleMap(): RoleMap {
  try {
    return parseRoleMap(localStorage.getItem(ROLES_KEY));
  } catch {
    return {};
  }
}

function saveRoleMap(map: RoleMap): void {
  try {
    localStorage.setItem(ROLES_KEY, JSON.stringify(map));
  } catch {
    /* localStorage may be unavailable */
  }
}

/** The role assigned to a template, or the default if none. */
export function getTemplateRole(templateId: string): TemplateRole {
  return loadRoleMap()[templateId] ?? DEFAULT_ROLE;
}

/** Persist a template's role assignment. */
export function setTemplateRole(templateId: string, role: TemplateRole): void {
  const map = loadRoleMap();
  map[templateId] = role;
  saveRoleMap(map);
}
