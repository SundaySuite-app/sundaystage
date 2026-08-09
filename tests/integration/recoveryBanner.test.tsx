// The recovery banner, at the seam where it can reach a LIVE projector.
//
// Core promise #1 is that the live output is sacrosanct. The crash-recovery
// banner used to be able to break it three ways at once: it rendered whenever
// a recoverable session existed — including while a service was running — its
// Discard button called `live_end`, which pushes `Black` to the outputs, and
// going live never cleared the offer, so the banner survived behind a running
// service. An `ErrorBoundary` reload mid-service (a panel throws, the webview
// reloads) is exactly the path that puts an operator one click away from a
// black projector in front of a congregation.
//
// These render the REAL `OperatorWorkspace` against a mocked IPC boundary, so
// what is asserted is the wiring, not a helper.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  render,
  screen,
  waitFor,
  cleanup,
  fireEvent,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import type { Library } from "@/lib/bindings";

// Everything the mock factory reads has to live inside `vi.hoisted`: the
// factory is lifted above the imports.
const { ipcMock } = vi.hoisted(() => {
  const recoverable = {
    service_id: "crashed-service",
    index: 3,
    total: 12,
    output: "normal",
    frame: { kind: "black" },
    log_len: 4,
    started_at: 1_000,
  };
  const liveNow = {
    ...recoverable,
    service_id: "todays-service",
    index: 0,
    started_at: 999_000,
  };
  const service = {
    id: "todays-service",
    library_id: "lib",
    name: "Gudstjeneste",
    starts_at: 0,
    notes: null,
    secondary_language: null,
    state: "planned",
    created_at: 0,
    updated_at: 0,
    deleted_at: null,
  };
  const ipcMock = {
    service: {
      upcoming: vi.fn(async () => [service]),
      items: vi.fn(async () => []),
      songsByItem: vi.fn(async () => ({})),
    },
    live: {
      // One cue, because Go Live is disabled on an empty queue.
      compileCueList: vi.fn(async () => ({
        service_id: "todays-service",
        compiled_at: 0,
        cues: [
          {
            kind: "show_slide",
            cue_id: "cue-1",
            slide_content: {
              section_label: "Vers 1",
              text_lines: ["Å store Gud"],
              translation_lines: null,
              reference: null,
              sensitive_slide: false,
              appearance: null,
            },
            theme_id: null,
            template_id: null,
            source: {
              service_item_id: "item-1",
              display_label: "Å store Gud",
              song_id: null,
            },
          },
        ],
      })),
      start: vi.fn(async () => liveNow),
      state: vi.fn(async () => liveNow),
      recover: vi.fn(async () => recoverable),
      end: vi.fn(async () => undefined),
      discardRecovery: vi.fn(async () => undefined),
      stagePresets: vi.fn(async () => []),
    },
    output: {
      config: vi.fn(async () => ({ assignments: [] })),
      open: vi.fn(async () => undefined),
      isOpen: vi.fn(async () => false),
      // The real default: the preview renders it, and `null` would throw
      // inside a component rather than in the code under test.
      appearance: vi.fn(async () => ({
        text_scale: 1.0,
        text_color: "#ffffff",
        bg_color: "#0a1730",
        h_align: "center",
        show_section_label: true,
        uppercase: false,
        line_height: 1.1,
      })),
      displayConfig: vi.fn(async () => null),
      monitors: vi.fn(async () => []),
    },
    // E6's consent card and problem-report dialog render in the same tree.
    // Stubbed so they run their real path rather than erroring into invisibility
    // — the banner assertions below must be about the banner, not about a
    // corner card that happened to throw.
    telemetry: {
      consent: {
        get: vi.fn(async () => ({
          status: "granted",
          version: 1,
          decidedAt: 0,
          currentVersion: 1,
          needsPrompt: false,
          active: true,
        })),
        set: vi.fn(async () => undefined),
      },
      queueStatus: vi.fn(async () => ({
        queued: 0,
        pendingReports: 0,
        lastError: null,
      })),
    },
    media: { list: vi.fn(async () => []) },
    sync: { status: vi.fn(async () => ({ state: "off" })) },
    search: { all: vi.fn(async () => []) },
    song: { list: vi.fn(async () => []) },
    deck: { list: vi.fn(async () => []) },
  };
  return { ipcMock };
});

vi.mock("@/lib/ipc", () => ({ ipc: ipcMock }));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => undefined),
  listen: vi.fn(async () => () => {}),
}));

// Imported after the mocks are registered.
import { OperatorWorkspace } from "@/features/workspace/OperatorWorkspace";

const LIBRARY = {
  id: "lib",
  name: "Menighet",
  default_locale: "no",
  default_theme_id: null,
  default_template_id: null,
  created_at: 0,
  updated_at: 0,
} as unknown as Library;

function renderWorkspace() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <OperatorWorkspace library={LIBRARY} />
    </QueryClientProvider>,
  );
}

/** The banner's two buttons, by their Norwegian catalog labels. */
const DISCARD = "Forkast";
const RESUME = "Gjenoppta";

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("the crash-recovery banner", () => {
  it("offers a resume when nothing is live", async () => {
    renderWorkspace();
    expect(await screen.findByText(DISCARD)).toBeInTheDocument();
    expect(screen.getByText(RESUME)).toBeInTheDocument();
  });

  it("clears the log on Discard — and never ends a service", async () => {
    renderWorkspace();
    fireEvent.click(await screen.findByText(DISCARD));

    expect(ipcMock.live.discardRecovery).toHaveBeenCalledTimes(1);
    expect(
      ipcMock.live.end,
      "`live_end` blacks the outputs — Discard must never call it",
    ).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.queryByText(DISCARD)).not.toBeInTheDocument(),
    );
  });

  it("goes away when the operator goes live, instead of hanging over the service", async () => {
    renderWorkspace();
    await screen.findByText(DISCARD);

    // Go Live is disabled until the cue list has compiled.
    const goLive = screen.getByText("Gå live");
    await waitFor(() => expect(goLive).not.toBeDisabled());
    fireEvent.click(goLive);
    await waitFor(() => expect(ipcMock.live.start).toHaveBeenCalled());

    await waitFor(() =>
      expect(
        screen.queryByText(DISCARD),
        "no recovery offer may sit on top of a live service",
      ).not.toBeInTheDocument(),
    );
    expect(screen.queryByText(RESUME)).not.toBeInTheDocument();
  });
});
