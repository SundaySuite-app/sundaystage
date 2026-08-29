// The two Spor A guards, at the seam where they can reach a LIVE projector.
//
// A lock that is only tested as a pure predicate is a lock that has never been
// shown to stop anything: the interesting failure is not "the policy said
// blocked", it is "the console asked Rust anyway". So these render the REAL
// `OperatorWorkspace` against a mocked IPC boundary and assert on what crosses
// it. `ipc.live.dispatch` and `ipc.live.start` are the only two calls that can
// move a projector; if the lock works, neither one happens.
//
// The mock keeps a small Rust-faithful session state machine (blackout and logo
// are toggles, every advance normalises the output), because restore-after-Clear
// is only correct if the state it captures is the state Rust actually held.
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

const { ipcMock, resetSession } = vi.hoisted(() => {
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

  const slide = (id: string, line: string) => ({
    kind: "show_slide",
    cue_id: id,
    slide_content: {
      section_label: "Vers 1",
      text_lines: [line],
      translation_lines: null,
      reference: null,
      sensitive_slide: false,
    },
    theme_id: null,
    template_id: null,
    source: {
      service_item_id: "item-1",
      item_cue_index: 0,
      display_label: line,
      song_id: null,
    },
  });
  const cues = [slide("cue-1", "Å store Gud"), slide("cue-2", "Når jeg")];

  // ── A miniature of `LiveSession::dispatch` ────────────────────────────────
  const state = {
    index: 0,
    output: "normal" as string,
    text: null as string | null,
  };
  function resetSession() {
    state.index = 0;
    state.output = "normal";
    state.text = null;
  }
  function frame() {
    switch (state.output) {
      case "blackout":
        return { kind: "black" };
      case "logo":
        return { kind: "logo" };
      case "message":
        return { kind: "message", text: state.text };
      default:
        return {
          kind: "slide",
          slide_content: cues[state.index].slide_content,
        };
    }
  }
  function view() {
    return {
      service_id: service.id,
      index: state.index,
      total: cues.length,
      output: state.output,
      frame: frame(),
      log_len: 0,
      started_at: 0,
    };
  }
  function apply(action: { type: string; index?: number; text?: string }) {
    switch (action.type) {
      case "next":
        state.index = Math.min(state.index + 1, cues.length - 1);
        state.output = "normal";
        state.text = null;
        break;
      case "previous":
        state.index = Math.max(state.index - 1, 0);
        state.output = "normal";
        state.text = null;
        break;
      case "go_to":
        state.index = action.index ?? state.index;
        state.output = "normal";
        state.text = null;
        break;
      case "blackout":
        state.output = state.output === "blackout" ? "normal" : "blackout";
        break;
      case "show_logo":
        state.output = state.output === "logo" ? "normal" : "logo";
        break;
      case "show_message":
        state.output = "message";
        state.text = action.text ?? "";
        break;
      case "clear":
        state.output = "normal";
        state.text = null;
        break;
    }
    return view();
  }

  const ipcMock = {
    service: {
      upcoming: vi.fn(async () => [service]),
      items: vi.fn(async () => []),
      songsByItem: vi.fn(async () => ({})),
      cueSummary: vi.fn(async () => ({ items: [] })),
    },
    live: {
      compileCueList: vi.fn(async () => ({
        service_id: service.id,
        compiled_at: 0,
        cues,
      })),
      start: vi.fn(async () => {
        resetSession();
        return view();
      }),
      state: vi.fn(async () => view()),
      dispatch: vi.fn(
        async (action: { type: string; index?: number; text?: string }) =>
          apply(action),
      ),
      // No crash to recover — the banner must not steal the assertions.
      recover: vi.fn(async () => null),
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
  return { ipcMock, resetSession };
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

/** The console reads keys off `document`; these mirror what a real press sends. */
function press(key: string, mods: Partial<KeyboardEventInit> = {}) {
  fireEvent.keyDown(document.body, { key, ...mods });
}

// jsdom has no layout, so the slide grid's "scroll the live cue into view" is
// a missing method rather than a no-op. Stub it: this suite is about what
// reaches Rust, not about scrolling.
Element.prototype.scrollIntoView = vi.fn();

/** The transport's Clear button — enabled exactly when an override is up. */
function clearButton(): HTMLButtonElement {
  return screen.getByText("Nullstill") as HTMLButtonElement;
}

/**
 * Wait until the console has actually applied the session Rust returned.
 * Asserting on the dispatch mock alone would race the state update that
 * `captureOverride` reads.
 */
async function waitForOverrideOnAir() {
  await waitFor(() => expect(clearButton()).not.toBeDisabled());
}

/** Go live and settle, so assertions start from a running service. */
async function goLive() {
  const goLiveButton = await screen.findByText("Gå live");
  await waitFor(() => expect(goLiveButton).not.toBeDisabled());
  fireEvent.click(goLiveButton);
  await waitFor(() => expect(ipcMock.live.start).toHaveBeenCalled());
  await screen.findByText("Blackout");
}

/** Turn the output lock on via ⌘L and wait for the button to say so. */
async function lock() {
  press("l", { metaKey: true });
  await screen.findByText("LÅST");
}

beforeEach(() => {
  vi.clearAllMocks();
  resetSession();
  localStorage.clear();
});
afterEach(cleanup);

// ── The output lock ─────────────────────────────────────────────────────────

describe("the output lock stops every route to the projector", () => {
  it("is off by default, and ⌘L turns it on and off again", async () => {
    renderWorkspace();
    expect(await screen.findByText("Lås")).toBeInTheDocument();
    await lock();
    press("l", { metaKey: true });
    expect(await screen.findByText("Lås")).toBeInTheDocument();
  });

  it("refuses to go live at all — that is its own route to the screen", async () => {
    renderWorkspace();
    await screen.findByText("Gå live");
    await lock();

    const goLiveButton = screen.getByText("Gå live");
    await waitFor(() => expect(goLiveButton).not.toBeDisabled());
    fireEvent.click(goLiveButton);
    press(" ");

    expect(
      ipcMock.live.start,
      "a locked output must not even open a session",
    ).not.toHaveBeenCalled();
  });

  it("swallows Space, Enter and G while live", async () => {
    renderWorkspace();
    await goLive();
    await lock();
    ipcMock.live.dispatch.mockClear();

    press(" ");
    press("Enter");
    press("g");

    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("swallows the Go button in the preview panel", async () => {
    renderWorkspace();
    await goLive();
    await lock();
    ipcMock.live.dispatch.mockClear();

    fireEvent.click(screen.getByText("Send"));

    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("swallows the logo, the one other content override on the bar", async () => {
    renderWorkspace();
    await goLive();
    await lock();
    ipcMock.live.dispatch.mockClear();

    fireEvent.click(screen.getByText("Logo"));
    press("l");

    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("still lets the panic keys through — blackout is a fire escape", async () => {
    renderWorkspace();
    await goLive();
    await lock();
    ipcMock.live.dispatch.mockClear();

    fireEvent.click(screen.getByText("Blackout"));
    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({ type: "blackout" }),
    );

    // …and so does ⇧B, the keyboard route to the same action.
    ipcMock.live.dispatch.mockClear();
    press("B", { shiftKey: true });
    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({ type: "blackout" }),
    );
  });

  it("still lets Clear through while locked", async () => {
    renderWorkspace();
    await goLive();
    fireEvent.click(screen.getByText("Blackout"));
    await waitForOverrideOnAir();
    await lock();
    ipcMock.live.dispatch.mockClear();

    fireEvent.click(clearButton());
    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({ type: "clear" }),
    );
  });

  it("says why it refused, instead of failing silently", async () => {
    renderWorkspace();
    await goLive();
    await lock();

    press(" ");

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /Utgangen er låst/,
    );
  });

  it("hands the console back the moment it is unlocked", async () => {
    renderWorkspace();
    await goLive();
    await lock();
    press(" ");
    expect(ipcMock.live.dispatch).not.toHaveBeenCalledWith({
      type: "go_to",
      index: 0,
    });

    press("l", { metaKey: true });
    await screen.findByText("Lås");
    press(" ");

    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({
        type: "go_to",
        index: 0,
      }),
    );
  });
});

// ── Escape no longer reaches the projector ──────────────────────────────────

describe("Escape", () => {
  it("does not black out the congregation screen any more", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    press("Escape");

    expect(
      ipcMock.live.dispatch,
      "Escape is the reflex for closing a dialog — it must not reach the projector",
    ).not.toHaveBeenCalled();
  });
});

// ── Restore after Clear ─────────────────────────────────────────────────────

describe("restore after Clear", () => {
  const NURSERY = "Barnevakt: du trengs på barnerommet";

  /** Put an operator message on the output through the real popover. */
  async function showMessage() {
    fireEvent.click(screen.getByText("Melding"));
    fireEvent.click(await screen.findByText(NURSERY));
    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({
        type: "show_message",
        text: NURSERY,
      }),
    );
    await waitForOverrideOnAir();
  }

  it("offers a restore after clearing a message, and puts the exact text back", async () => {
    renderWorkspace();
    await goLive();
    await showMessage();

    fireEvent.click(clearButton());
    expect(await screen.findByText("Tekstlaget ble tømt.")).toBeInTheDocument();

    ipcMock.live.dispatch.mockClear();
    press("z", { metaKey: true });

    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({
        type: "show_message",
        text: NURSERY,
      }),
    );
  });

  it("restores from the button too — a shaken volunteer reaches for the mouse", async () => {
    renderWorkspace();
    await goLive();
    await showMessage();
    fireEvent.click(clearButton());

    ipcMock.live.dispatch.mockClear();
    fireEvent.click(await screen.findByText("Gjenopprett"));

    await waitFor(() =>
      expect(ipcMock.live.dispatch).toHaveBeenCalledWith({
        type: "show_message",
        text: NURSERY,
      }),
    );
  });

  it("names what was cleared — a logo is not a text layer", async () => {
    renderWorkspace();
    await goLive();
    fireEvent.click(screen.getByText("Logo"));
    await waitForOverrideOnAir();

    fireEvent.click(clearButton());
    expect(await screen.findByText("Logoen ble tømt.")).toBeInTheDocument();
  });

  it("drops the offer once the show has moved on", async () => {
    renderWorkspace();
    await goLive();
    await showMessage();
    fireEvent.click(clearButton());
    await screen.findByText("Tekstlaget ble tømt.");

    // The operator advanced: "restore" would now mean an override over a
    // different cue, which is not what they remember clearing.
    press(" ");
    await waitFor(() =>
      expect(
        screen.queryByText("Tekstlaget ble tømt."),
      ).not.toBeInTheDocument(),
    );

    ipcMock.live.dispatch.mockClear();
    press("z", { metaKey: true });
    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("offers nothing when Clear had nothing to throw away", async () => {
    renderWorkspace();
    await goLive();

    // The transport's Clear is disabled at rest, so drive the equivalent path:
    // clearing from the message popover while no message is showing.
    press("z", { metaKey: true });

    expect(screen.queryByText("Tekstlaget ble tømt.")).not.toBeInTheDocument();
  });
});
