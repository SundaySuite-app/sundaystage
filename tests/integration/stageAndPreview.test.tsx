// A5 (the stage screen under blackout) and the preview-dimming fix.
//
// Both are "what the operator and the band can actually see" changes, so they
// are asserted against the rendered DOM rather than a helper.
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";

import type {
  Cue,
  LiveFrame,
  LiveSessionView,
  OutputAppearance,
  StageDisplayConfig,
} from "@/lib/bindings";
import { StageDisplay } from "@/features/live/StageDisplay";
import { PreviewLivePanel } from "@/features/workspace/PreviewLivePanel";

const APPEARANCE: OutputAppearance = {
  text_scale: 1.0,
  text_color: "#ffffff",
  bg_color: "#0a1730",
  h_align: "center",
  show_section_label: true,
  uppercase: false,
  line_height: 1.1,
};

function slideCue(id: string, line: string): Cue {
  return {
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
    },
  };
}

const CUES = [slideCue("cue-1", "Å store Gud"), slideCue("cue-2", "Når jeg")];
const WORDS: LiveFrame = {
  kind: "slide",
  slide_content:
    CUES[0].kind === "show_slide" ? CUES[0].slide_content : ({} as never),
};
const BLACK: LiveFrame = { kind: "black" };

const PRESET: StageDisplayConfig = {
  id: "leader",
  name: "Lovsangsleder",
  show_current_slide: true,
  show_next_slide: true,
  lyrics_large: true,
  show_section_label: true,
  show_clock: false,
  show_service_timer: false,
  show_notes: false,
};

const SESSION: LiveSessionView = {
  service_id: "svc",
  index: 0,
  total: CUES.length,
  output: "blackout",
  frame: BLACK,
  log_len: 1,
  started_at: 0n,
};

afterEach(cleanup);

// ── A5 — the stage screen under a blackout ──────────────────────────────────

describe("the stage screen during a blackout", () => {
  it("keeps the words for the band, and says the room is dark", () => {
    render(
      <StageDisplay
        session={SESSION}
        stageFrame={WORDS}
        mainBlackedOut
        cues={CUES}
        serviceName="Gudstjeneste"
        notes={null}
        preset={PRESET}
        presets={[PRESET]}
        onPreset={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText("Å store Gud")).toBeInTheDocument();
    expect(screen.getByText("Salen er svartlagt")).toBeInTheDocument();
    expect(screen.queryByText("BLACKOUT")).not.toBeInTheDocument();
  });

  it("goes dark with the room when the church asked it to", () => {
    render(
      <StageDisplay
        session={SESSION}
        stageFrame={BLACK}
        mainBlackedOut
        cues={CUES}
        serviceName="Gudstjeneste"
        notes={null}
        preset={PRESET}
        presets={[PRESET]}
        onPreset={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText("BLACKOUT")).toBeInTheDocument();
    // No badge: nothing is being withheld from the band that the room can see.
    expect(screen.queryByText("Salen er svartlagt")).not.toBeInTheDocument();
  });
});

// ── The preview is never dimmed ─────────────────────────────────────────────

describe("Preview and Program are equals", () => {
  it("does not dim the preview while a service is live", () => {
    const { container } = render(
      <PreviewLivePanel
        cues={CUES}
        appearance={APPEARANCE}
        previewIndex={1}
        liveFrame={WORDS}
        liveIndex={0}
        isLive
        notes={null}
        onGo={vi.fn()}
      />,
    );

    // The staging monitor used to fade to 60 % exactly when it mattered most.
    const dimmed = container.querySelectorAll(
      '[class*="opacity-6"], [class*="opacity-5"], [class*="opacity-7"]',
    );
    expect(
      Array.from(dimmed).map((el) => el.className),
      "no pane in the Preview/Program stack may be dimmed while live",
    ).toEqual([]);
  });
});
