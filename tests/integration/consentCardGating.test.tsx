// The consent card's second live gate: a PENDING RECOVERY OFFER.
//
// `TelemetryConsentCard` refuses to render while `isLive` is true, which reads
// like the whole promise ("never during a service") and is not. After an
// ErrorBoundary reload mid-service — a panel throws, the webview reloads, the
// projector keeps showing the cue it was on — the frontend has NO session until
// the operator answers the recovery banner. `isLive` is false for that entire
// window, so the card alone would slide a privacy question into the corner of a
// running service, beside the banner asking whether to resume it.
//
// So the workspace gates it on `recoverable === null` as well. These render the
// REAL `OperatorWorkspace` against a mocked IPC boundary, with a positive
// control on the same tree, so what is asserted is the wiring rather than a
// helper.
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
      compileCueList: vi.fn(async () => ({
        service_id: "todays-service",
        compiled_at: 0,
        cues: [],
      })),
      start: vi.fn(async () => null),
      state: vi.fn(async () => null),
      recover: vi.fn(async () => recoverable),
      end: vi.fn(async () => undefined),
      discardRecovery: vi.fn(async () => undefined),
      stagePresets: vi.fn(async () => []),
    },
    output: {
      config: vi.fn(async () => ({ assignments: [] })),
      open: vi.fn(async () => undefined),
      isOpen: vi.fn(async () => false),
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
    // The state this whole file is about: an install that has NEVER been asked,
    // so the card wants to render and only the gates can stop it.
    telemetry: {
      consent: {
        get: vi.fn(async () => ({
          status: "never-asked",
          version: null,
          decidedAt: null,
          currentVersion: 1,
          needsPrompt: true,
          active: false,
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
import { CATALOG, useLocale } from "@/lib/i18n";
import { markTutorialSeen } from "@/lib/tutorial";

const LIBRARY = {
  id: "lib",
  name: "Menighet",
  default_locale: "no",
  default_theme_id: null,
  default_template_id: null,
  created_at: 0,
  updated_at: 0,
} as unknown as Library;

/** The card's accessible name, in whichever locale the store resolved to. */
const CONSENT_TITLE = CATALOG[useLocale.getState().lang].telConsentTitle;
const DISCARD = CATALOG[useLocale.getState().lang].recoveryDiscard;

function consentCard() {
  return screen.queryByRole("region", { name: CONSENT_TITLE });
}

function renderWorkspace() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <OperatorWorkspace library={LIBRARY} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  // The card's own mid-tour gate is a separate rule; this file is about the
  // recovery one, so the tutorial is already behind us.
  markTutorialSeen();
});
afterEach(cleanup);

describe("the consent card and a pending recovery offer", () => {
  it("stays hidden while a recovery offer is waiting to be answered", async () => {
    renderWorkspace();
    // The banner is up: a service may still be on that projector, and the
    // frontend cannot know until the operator says.
    await screen.findByText(DISCARD);

    // The consent query has resolved with `needsPrompt`, so the only thing
    // keeping the card away is the gate under test.
    await waitFor(() =>
      expect(ipcMock.telemetry.consent.get).toHaveBeenCalled(),
    );
    expect(
      consentCard(),
      "a privacy question must not appear next to an unanswered recovery offer",
    ).not.toBeInTheDocument();
  });

  it("appears once the offer is answered — so the gate is not just hiding it forever", async () => {
    renderWorkspace();
    fireEvent.click(await screen.findByText(DISCARD));

    await waitFor(() =>
      expect(ipcMock.live.discardRecovery).toHaveBeenCalledTimes(1),
    );
    await waitFor(() => expect(consentCard()).toBeInTheDocument());
  });
});
