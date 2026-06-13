import { describe, expect, it } from "vitest";
import { liveFrameToWebFrame } from "@/lib/webShare";
import type { LiveFrame, OutputAppearance, SlideContent } from "@/lib/bindings";

const appearance: OutputAppearance = {
  text_scale: 1.25,
  text_color: "#ffffff",
  bg_color: "#0a1730",
  h_align: "center",
  show_section_label: true,
  uppercase: false,
  line_height: 1.1,
};

function slide(over: Partial<SlideContent> = {}): LiveFrame {
  return {
    kind: "slide",
    slide_content: {
      section_label: "Vers 1",
      text_lines: ["Stor er din trofasthet"],
      translation_lines: null,
      reference: null,
      sensitive_slide: false,
      ...over,
    },
  };
}

describe("liveFrameToWebFrame", () => {
  it("maps a lyric slide and threads appearance into the web shape", () => {
    const wf = liveFrameToWebFrame(slide(), appearance);
    expect(wf).toMatchObject({
      v: 1,
      kind: "slide",
      text_lines: ["Stor er din trofasthet"],
      section_label: "Vers 1",
    });
    expect(wf.appearance).toEqual({
      bg_color: "#0a1730",
      text_color: "#ffffff",
      font_scale: 1.25,
    });
  });

  it("gates a sensitive slide to the neutral placeholder (private text never leaves)", () => {
    const wf = liveFrameToWebFrame(
      slide({ text_lines: ["Forbønn: Navn Navnesen"], sensitive_slide: true }),
      appearance,
    );
    expect(wf.kind).toBe("message");
    expect(wf.message).toBe("Tjeneste pågår");
    expect(JSON.stringify(wf)).not.toContain("Navnesen");
  });

  it("maps black/logo/message", () => {
    expect(liveFrameToWebFrame({ kind: "black" }).kind).toBe("black");
    expect(liveFrameToWebFrame({ kind: "logo" }).kind).toBe("logo");
    expect(
      liveFrameToWebFrame({ kind: "message", text: "Velkommen" }),
    ).toMatchObject({
      kind: "message",
      message: "Velkommen",
    });
  });

  it("carries scripture reference + translation lines", () => {
    const wf = liveFrameToWebFrame(
      slide({ reference: "Joh 3,16", translation_lines: ["For God so loved"] }),
    );
    expect(wf.reference).toBe("Joh 3,16");
    expect(wf.translation_lines).toEqual(["For God so loved"]);
  });
});
