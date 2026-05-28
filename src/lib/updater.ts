/**
 * Auto-update helpers (Phase 13.2).
 *
 * Thin wrappers over the Tauri updater + process plugins. `checkForUpdate`
 * reaches the GitHub Releases `latest.json` manifest configured in
 * tauri.conf.json, verifies the bundle's signature against the embedded public
 * key, and reports whether a newer signed build exists.
 *
 * Everything is guarded: in a plain browser (vite dev without Tauri), offline,
 * or before the first release exists, `checkForUpdate` resolves to null instead
 * of throwing — so the app never breaks on a failed update check.
 */
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type { Update };

export async function checkForUpdate(): Promise<Update | null> {
  try {
    return await check();
  } catch {
    // Not a Tauri desktop context, offline, or no manifest published yet.
    return null;
  }
}

/** Download + install the update, then relaunch into the new version. */
export async function installAndRelaunch(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
