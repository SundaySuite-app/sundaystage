// updater — the frontend half of the ring seam (E2).
//
// The property that matters on a Sunday: an update check that does not produce
// an update must be *quiet*. The ring answers 204 when nothing is promoted or
// the ring is paused — the Rust command turns that into `null`, exactly like
// "already newest" — and any genuine failure (offline, DNS, no Tauri runtime
// at all) must also resolve to null rather than throw, because `UpdateBanner`
// runs this on every launch.
//
// The install path is pinned too: install first, relaunch second, and a failed
// install must not relaunch into the old build's half-applied state.
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { UpdateInfo } from "@/lib/bindings";

const calls: string[] = [];
let checkImpl: () => Promise<UpdateInfo | null> = async () => null;
let installImpl: () => Promise<void> = async () => {};
let relaunchImpl: () => Promise<void> = async () => {};

vi.mock("@/lib/ipc", () => ({
  ipc: {
    update: {
      check: () => {
        calls.push("check");
        return checkImpl();
      },
      install: () => {
        calls.push("install");
        return installImpl();
      },
    },
  },
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: () => {
    calls.push("relaunch");
    return relaunchImpl();
  },
}));

// Imported after the mocks are registered.
import { checkForUpdate, installAndRelaunch } from "@/lib/updater";

const OFFER: UpdateInfo = {
  version: "0.5.0",
  current_version: "0.4.0",
  channel: "stable",
  notes: "Rings.",
};

describe("checkForUpdate", () => {
  beforeEach(() => {
    calls.length = 0;
    checkImpl = async () => null;
  });

  it("passes an offer through", async () => {
    checkImpl = async () => OFFER;
    await expect(checkForUpdate()).resolves.toEqual(OFFER);
  });

  it("resolves to null when the ring has nothing promoted (204 → null)", async () => {
    checkImpl = async () => null;
    await expect(checkForUpdate()).resolves.toBeNull();
  });

  it("resolves to null instead of throwing when the check itself fails", async () => {
    // Offline, DNS failure, or a plain browser with no Tauri IPC at all.
    checkImpl = async () => {
      throw new Error("network unreachable");
    };
    await expect(checkForUpdate()).resolves.toBeNull();
  });
});

describe("installAndRelaunch", () => {
  beforeEach(() => {
    calls.length = 0;
    installImpl = async () => {};
    relaunchImpl = async () => {};
  });

  it("installs before relaunching", async () => {
    await installAndRelaunch();
    expect(calls).toEqual(["install", "relaunch"]);
  });

  it("does not relaunch when the install fails", async () => {
    installImpl = async () => {
      throw new Error("signature mismatch");
    };
    await expect(installAndRelaunch()).rejects.toThrow("signature mismatch");
    expect(calls).toEqual(["install"]);
  });
});
