// The update banner must say WHICH ring it is offering.
//
// The install path refuses an offer from a ring the install no longer follows
// (see `commands::updater`), and this is the half of that rule the operator can
// read: a church that has just moved back to stable should be able to see that
// the banner still on screen is the beta build, rather than clicking Download
// and being told no.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";

import type { UpdateInfo } from "@/lib/bindings";

const { updaterMock } = vi.hoisted(() => ({
  updaterMock: {
    checkForUpdate: vi.fn(async (): Promise<UpdateInfo | null> => null),
    installAndRelaunch: vi.fn(async () => undefined),
  },
}));

vi.mock("@/lib/updater", () => updaterMock);

// Imported after the mock is registered.
import { UpdateBanner } from "@/components/UpdateBanner";

const OFFER: UpdateInfo = {
  version: "0.6.0-beta.1",
  current_version: "0.5.0",
  channel: "beta",
  notes: null,
};

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("UpdateBanner", () => {
  it("names the beta ring an offer came from", async () => {
    updaterMock.checkForUpdate.mockResolvedValueOnce(OFFER);
    render(<UpdateBanner />);

    expect(await screen.findByText(/0\.6\.0-beta\.1/)).toBeInTheDocument();
    expect(screen.getByText("Fra Beta-kanalen.")).toBeInTheDocument();
  });

  it("names the stable ring too — the ring is always stated, never implied", async () => {
    updaterMock.checkForUpdate.mockResolvedValueOnce({
      ...OFFER,
      version: "0.6.0",
      channel: "stable",
    });
    render(<UpdateBanner />);

    expect(await screen.findByText("Fra Stabil-kanalen.")).toBeInTheDocument();
  });

  it("stays out of the way when the ring has nothing promoted", async () => {
    updaterMock.checkForUpdate.mockResolvedValueOnce(null);
    const { container } = render(<UpdateBanner />);
    expect(container).toBeEmptyDOMElement();
  });
});
