// Section-jump key sequences (Spor A4) at the seam where they reach a LIVE
// projector.
//
// `sectionJump.test.ts` proves the resolver picks the right section;
// `consoleKeys.test.ts` proves the key table answers the right action. Neither
// proves the console actually asks Rust to move — or, more importantly, that a
// LOCKED output stops it from asking. A jump is a route to the congregation
// screen like every other one, so it has to funnel through the same `dispatch`
// the lock guards. That is what this file renders the real `OperatorWorkspace`
// to check.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  render,
  screen,
  waitFor,
  cleanup,
  fireEvent,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import type { Library } from "@/lib/bindings";
import { SECTION_SEQ_IDLE_MS } from "@/features/workspace/sectionJump";

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

  const slide = (item: string, label: string, n: number) => ({
    kind: "show_slide",
    cue_id: `${item}-${n}`,
    slide_content: {
      section_label: label,
      text_lines: [`${label} linje`],
      translation_lines: null,
      reference: null,
      sensitive_slide: false,
    },
    theme_id: null,
    template_id: null,
    source: {
      service_item_id: item,
      item_cue_index: n,
      display_label: `Sang — ${label}`,
      song_id: null,
    },
  });

  // A real arrangement: the chorus is one section the song plays three times,
  // and there is a second song after it so the scoping has something to get
  // wrong. Labels are what the Rust cue compiler actually emits — humanized
  // canonical English — which the Norwegian UI shows as Vers/Refreng/Bro.
  const cues = [
    slide("song-1", "Verse 1", 0), //  0
    slide("song-1", "Verse 1", 1), //  1
    slide("song-1", "Chorus", 2), //   2
    slide("song-1", "Verse 2", 3), //  3
    slide("song-1", "Chorus", 4), //   4
    slide("song-1", "Bridge", 5), //   5
    slide("song-1", "Chorus", 6), //   6
    slide("song-2", "Verse 1", 0), //  7
  ];

  const state = { index: 0, output: "normal" as string };
  function resetSession() {
    state.index = 0;
    state.output = "normal";
  }
  function view() {
    return {
      service_id: service.id,
      index: state.index,
      total: cues.length,
      output: state.output,
      frame:
        state.output === "blackout"
          ? { kind: "black" }
          : { kind: "slide", slide_content: cues[state.index].slide_content },
      log_len: 0,
      started_at: 0,
    };
  }
  function apply(action: { type: string; index?: number }) {
    switch (action.type) {
      case "next":
        state.index = Math.min(state.index + 1, cues.length - 1);
        state.output = "normal";
        break;
      case "previous":
        state.index = Math.max(state.index - 1, 0);
        state.output = "normal";
        break;
      case "go_to":
        state.index = action.index ?? state.index;
        state.output = "normal";
        break;
      case "blackout":
        state.output = state.output === "blackout" ? "normal" : "blackout";
        break;
      case "clear":
        state.output = "normal";
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
      dispatch: vi.fn(async (action: { type: string; index?: number }) =>
        apply(action),
      ),
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

/** Type a sequence the way an operator does — one key after another. */
function type(seq: string) {
  for (const ch of seq) press(ch);
}

Element.prototype.scrollIntoView = vi.fn();

async function goLive() {
  const goLiveButton = await screen.findByText("Gå live");
  await waitFor(() => expect(goLiveButton).not.toBeDisabled());
  fireEvent.click(goLiveButton);
  await waitFor(() => expect(ipcMock.live.start).toHaveBeenCalled());
  await screen.findByText("Blackout");
}

/** Every `go_to` index the console asked Rust for, in order. */
function jumps(): number[] {
  return ipcMock.live.dispatch.mock.calls
    .map(([a]) => a as { type: string; index?: number })
    .filter((a) => a.type === "go_to")
    .map((a) => a.index!);
}

function chip(): HTMLElement | null {
  return screen.queryByTestId("section-seq");
}

beforeEach(() => {
  vi.clearAllMocks();
  resetSession();
  localStorage.clear();
});
afterEach(cleanup);

describe("a typed sequence moves the show", () => {
  it("V2 puts verse 2 on air", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    type("v2");

    await waitFor(() => expect(jumps()).toEqual([3]));
  });

  it("R takes the chorus the band is heading for, not the first one", async () => {
    renderWorkspace();
    await goLive();
    type("v2"); // live on verse 2 (index 3)
    await waitFor(() => expect(jumps()).toEqual([3]));

    type("r");

    // Index 2 is also a chorus and would show the identical words — and then
    // "next" would replay verse 2. The one ahead is the one that is meant.
    await waitFor(() => expect(jumps()).toEqual([3, 4]));
  });

  it("stages the slide after the jump, the way Go does", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    type("v2");
    await waitFor(() => expect(jumps()).toEqual([3]));
    press(" "); // Go promotes what the jump staged

    await waitFor(() => expect(jumps()).toEqual([3, 4]));
  });

  it("keeps each song's sections to itself", async () => {
    renderWorkspace();
    await goLive();
    // Walk into the second song (its Verse 1 is the last cue in the service).
    press("End");
    press(" ");
    await waitFor(() => expect(jumps()).toEqual([7]));
    ipcMock.live.dispatch.mockClear();

    // Song 2 has exactly one verse, so `V` is unambiguous *here* — while the
    // same key in song 1 would have had to wait between two verses. If the
    // scope leaked to the whole service it could not know that, and it would
    // land on song 1's verse 1 at index 0.
    press("v");

    await waitFor(() => expect(jumps()).toEqual([7]));
  });
});

describe("the operator can see what they are typing", () => {
  it("echoes the sequence and names the section it points at", async () => {
    renderWorkspace();
    await goLive();

    press("v"); // song 1 has two verses, so this one waits

    const shown = await screen.findByTestId("section-seq");
    expect(shown).toHaveTextContent("v");
    expect(
      shown,
      "the section reads in the operator's language",
    ).toHaveTextContent("Vers 1");
    expect(jumps(), "an unfinished sequence must not move anything").toEqual(
      [],
    );
  });

  it("says so when the sequence matches no section here", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    press("q");

    expect(await screen.findByText("ingen seksjon her")).toBeInTheDocument();
    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("Escape drops the sequence and touches nothing", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    press("v");
    await screen.findByTestId("section-seq");
    press("Escape");

    await waitFor(() => expect(chip()).toBeNull());
    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("Enter takes the jump a waiting sequence is offering", async () => {
    renderWorkspace();
    await goLive();
    type("v2");
    await waitFor(() => expect(jumps()).toEqual([3]));
    ipcMock.live.dispatch.mockClear();

    press("v"); // waits: verse 1 or verse 2?
    await screen.findByTestId("section-seq");
    press("Enter");

    await waitFor(() => expect(jumps()).toEqual([0]));
    await waitFor(() => expect(chip()).toBeNull());
  });

  it("gives Enter straight back to Go once no sequence stands", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    press("Enter");

    // Go, not a jump: `go_to` at the staged slide, with no sequence involved.
    await waitFor(() => expect(ipcMock.live.dispatch).toHaveBeenCalled());
    expect(chip()).toBeNull();
  });

  it("clears a half-typed sequence on its own after a beat", async () => {
    renderWorkspace();
    await goLive();
    ipcMock.live.dispatch.mockClear();

    // Fake timers only for the lapse itself: a forgotten `V` left standing
    // would silently change what the NEXT keystroke means.
    vi.useFakeTimers();
    try {
      press("v");
      expect(chip()).toBeTruthy();
      act(() => {
        vi.advanceTimersByTime(SECTION_SEQ_IDLE_MS + 200);
      });
      expect(chip()).toBeNull();
    } finally {
      vi.useRealTimers();
    }
    expect(
      ipcMock.live.dispatch,
      "lapsing is not a jump",
    ).not.toHaveBeenCalled();
  });

  it("shows nothing at all off air", async () => {
    renderWorkspace();
    await screen.findByText("Gå live");

    type("v2");

    expect(chip()).toBeNull();
    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });
});

describe("the output lock catches a section jump like any other route", () => {
  it("a locked output never asks Rust to move", async () => {
    renderWorkspace();
    await goLive();
    press("l", { metaKey: true });
    await screen.findByText("LÅST");
    ipcMock.live.dispatch.mockClear();

    type("v2");
    type("r");

    // The jump is a `go_to`, the lock's business exactly. If this ever fails by
    // reaching Rust, someone has invented a second route to the projector.
    await waitFor(() => expect(screen.getByText("LÅST")).toBeInTheDocument());
    expect(ipcMock.live.dispatch).not.toHaveBeenCalled();
  });

  it("says why, instead of failing silently over a live screen", async () => {
    renderWorkspace();
    await goLive();
    press("l", { metaKey: true });
    await screen.findByText("LÅST");

    type("v2");

    expect(await screen.findByText(/Utgangen er låst/i)).toBeInTheDocument();
  });

  it("lets the jump through again the moment the lock comes off", async () => {
    renderWorkspace();
    await goLive();
    press("l", { metaKey: true });
    await screen.findByText("LÅST");
    type("v2");
    press("l", { metaKey: true });
    await screen.findByText("Lås");
    ipcMock.live.dispatch.mockClear();

    type("v2");

    await waitFor(() => expect(jumps()).toEqual([3]));
  });
});
