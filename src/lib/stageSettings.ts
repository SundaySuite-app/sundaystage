/**
 * Stage-screen behaviour (Spor A5) — a per-device operator preference.
 *
 * When the operator blacks out the congregation screen, should the *stage*
 * screen go dark too? Almost never: the band is mid-song and still needs the
 * words. Blackout is aimed at the room, not at the musicians. So the default is
 * **off** — the stage keeps the text, with a small badge telling the band the
 * room is dark so nobody wonders whether the projector died.
 *
 * Churches that use one shared screen for both roles want the opposite, so it
 * is a flag rather than a rule. It lives in localStorage next to the locale and
 * the theme: a device preference, not synced data, and nothing that has to
 * survive a reinstall.
 */
import { create } from "zustand";

import type { LiveFrame } from "@/lib/bindings";

const STORAGE_KEY = "ss-stage-follows-blackout";

function initialFollowsBlackout(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    /* localStorage may be unavailable */
    return false;
  }
}

interface StageSettingsState {
  /** The stage screen goes black together with the main output. Default off. */
  followsBlackout: boolean;
  setFollowsBlackout: (v: boolean) => void;
}

export const useStageSettings = create<StageSettingsState>((set) => ({
  followsBlackout: initialFollowsBlackout(),
  setFollowsBlackout: (v) => {
    try {
      if (v) localStorage.setItem(STORAGE_KEY, "1");
      else localStorage.removeItem(STORAGE_KEY);
    } catch {
      /* ignore */
    }
    set({ followsBlackout: v });
  },
}));

/**
 * What the stage/confidence screens should show, given what the main output is
 * showing.
 *
 * Only a blackout separates them: everything else (a slide, the logo, an
 * operator message) is the same picture on every screen. When the stage does
 * not follow the blackout, it falls back to `underlying` — the frame the
 * running cue would render if nothing were overriding it.
 *
 * Pure and total: a missing `underlying` (no cue list yet) means the stage has
 * nothing better to show, so it follows the main output rather than blanking on
 * its own.
 */
export function stageFrameFor(
  main: LiveFrame | null,
  underlying: LiveFrame | null,
  followsBlackout: boolean,
): LiveFrame | null {
  if (!main) return null;
  if (followsBlackout || main.kind !== "black") return main;
  return underlying ?? main;
}
