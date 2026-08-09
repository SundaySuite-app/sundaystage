/**
 * Auto-update helpers (Phase 13.2; rings since E2).
 *
 * Up to v0.4.0 this called the updater plugin's JS `check()`, which always uses
 * the endpoints baked into `tauri.conf.json` — its `CheckOptions` has no
 * `endpoints` field. Since the operator can choose an update ring
 * (stable/beta), the check moved to Rust, where
 * `UpdaterBuilder::endpoints(..)` can pick the ring per check. These wrappers
 * now go through `ipc.update.*`; the signature verification, download and
 * install underneath are still the updater plugin's own code — its *Rust* half,
 * which is why only that half is a dependency and the JS package is not
 * installed at all.
 *
 * Everything stays guarded: in a plain browser (vite dev without Tauri),
 * offline, or before anything is promoted to the ring, `checkForUpdate`
 * resolves to null instead of throwing — so the app never breaks on a failed
 * update check. A ring that has nothing promoted (or is paused) answers 204,
 * which the updater reports as "up to date", not as an error.
 */
import { relaunch } from "@tauri-apps/plugin-process";

import { ipc } from "@/lib/ipc";
import type { UpdateInfo } from "@/lib/bindings";

export type { UpdateInfo };

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    return await ipc.update.check();
  } catch {
    // Not a Tauri desktop context, offline, or the ring was unreachable.
    return null;
  }
}

/** Download + install the update, then relaunch into the new version. */
export async function installAndRelaunch(): Promise<void> {
  await ipc.update.install();
  await relaunch();
}
