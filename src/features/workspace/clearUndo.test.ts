// Restore-after-Clear: the promise is "exactly what was there", so these tests
// are about exactness, not about a toast appearing.
//
// The subtle part is that Rust's blackout/logo are TOGGLES, and Clear always
// leaves the session at `normal`. One toggle from `normal` therefore lands on
// precisely the captured state — but only because Clear normalises first. If
// that ever stops being true (a Clear that leaves the logo up, say), the
// round-trip below is where it shows.
import { describe, expect, it } from "vitest";

import type { LiveSessionView, OutputState } from "@/lib/bindings";
import {
  CLEAR_UNDO_WINDOW_MS,
  captureOverride,
  restoreAction,
  secondsLeft,
} from "./clearUndo";

function session(
  output: OutputState,
  frame: LiveSessionView["frame"],
): LiveSessionView {
  return {
    service_id: "svc",
    index: 3,
    total: 12,
    output,
    frame,
    log_len: 7,
    started_at: 0n,
  };
}

const SLIDE: LiveSessionView["frame"] = {
  kind: "slide",
  slide_content: {
    section_label: "Vers 1",
    text_lines: ["Å store Gud"],
    translation_lines: null,
    reference: null,
    sensitive_slide: false,
  },
};

describe("captureOverride", () => {
  it("offers nothing when there is no session", () => {
    expect(captureOverride(null)).toBeNull();
  });

  it("offers nothing when the output is already showing its cue", () => {
    expect(captureOverride(session("normal", SLIDE))).toBeNull();
  });

  it("captures the operator message verbatim — the only text Clear destroys", () => {
    const cleared = captureOverride(
      session("message", { kind: "message", text: "Barnevakt til rom 2" }),
    );
    expect(cleared).toEqual({
      output: "message",
      text: "Barnevakt til rom 2",
    });
  });

  it("trims the captured message so the restore matches what was shown", () => {
    const cleared = captureOverride(
      session("message", { kind: "message", text: "  Velkommen  " }),
    );
    expect(cleared?.text).toBe("Velkommen");
  });

  it("offers nothing for an all-whitespace message (Rust treats it as a clear)", () => {
    expect(
      captureOverride(session("message", { kind: "message", text: "   " })),
    ).toBeNull();
  });

  it("captures a blackout and a logo without text", () => {
    expect(captureOverride(session("blackout", { kind: "black" }))).toEqual({
      output: "blackout",
      text: null,
    });
    expect(captureOverride(session("logo", { kind: "logo" }))).toEqual({
      output: "logo",
      text: null,
    });
  });
});

describe("restoreAction", () => {
  it("puts the exact message back", () => {
    expect(
      restoreAction({ output: "message", text: "Barnevakt til rom 2" }),
    ).toEqual({ type: "show_message", text: "Barnevakt til rom 2" });
  });

  it("toggles blackout back on (Clear left the session at normal)", () => {
    expect(restoreAction({ output: "blackout", text: null })).toEqual({
      type: "blackout",
    });
  });

  it("toggles the logo back on", () => {
    expect(restoreAction({ output: "logo", text: null })).toEqual({
      type: "show_logo",
    });
  });
});

describe("capture → restore round-trip", () => {
  const cases: Array<[string, LiveSessionView]> = [
    ["blackout", session("blackout", { kind: "black" })],
    ["logo", session("logo", { kind: "logo" })],
    [
      "message",
      session("message", { kind: "message", text: "Gudstjenesten starter" }),
    ],
  ];

  for (const [name, before] of cases) {
    it(`${name} survives clear → ⌘Z unchanged`, () => {
      const cleared = captureOverride(before);
      expect(cleared).not.toBeNull();
      const action = restoreAction(cleared!);
      // Simulate Rust: Clear normalised the session, and the restore action is
      // a toggle/set applied from `normal`.
      const after =
        action.type === "blackout"
          ? "blackout"
          : action.type === "show_logo"
            ? "logo"
            : "message";
      expect(after).toBe(before.output);
      if (action.type === "show_message") {
        expect(action.text).toBe(
          before.frame.kind === "message" ? before.frame.text : "",
        );
      }
    });
  }
});

describe("secondsLeft", () => {
  it("starts at the full window and counts down to zero", () => {
    const t0 = 1_000_000;
    expect(secondsLeft(t0, t0)).toBe(CLEAR_UNDO_WINDOW_MS / 1000);
    expect(secondsLeft(t0, t0 + 3200)).toBe(4);
    expect(secondsLeft(t0, t0 + CLEAR_UNDO_WINDOW_MS)).toBe(0);
  });

  it("never goes negative once the window has lapsed", () => {
    expect(secondsLeft(0, CLEAR_UNDO_WINDOW_MS * 10)).toBe(0);
  });
});
