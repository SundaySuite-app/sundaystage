/**
 * Theme (light/dark) UI state — Zustand store + DOM application.
 *
 * The app is dark-first: the default tokens are dark, the `prefers-color-scheme`
 * media query lights them up, and a `.light` / `.dark` class on <html> lets the
 * user force a mode. So:
 *   - "system" → no class (the media query decides)
 *   - "light"  → `.light` class
 *   - "dark"   → `.dark` class (force dark even if the OS is light)
 */
import { create } from "zustand";

export type ThemeMode = "system" | "light" | "dark";

const KEY = "sundaystage.theme";

function applyToDom(mode: ThemeMode): void {
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  if (mode === "light") root.classList.add("light");
  else if (mode === "dark") root.classList.add("dark");
}

function initialMode(): ThemeMode {
  const saved = localStorage.getItem(KEY);
  return saved === "light" || saved === "dark" || saved === "system"
    ? saved
    : "system";
}

interface ThemeStore {
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
}

export const useTheme = create<ThemeStore>((set) => ({
  mode: initialMode(),
  setMode: (mode) => {
    localStorage.setItem(KEY, mode);
    applyToDom(mode);
    set({ mode });
  },
}));

// Apply the persisted choice immediately on load to avoid a flash.
applyToDom(useTheme.getState().mode);
