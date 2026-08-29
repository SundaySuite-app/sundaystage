// The output lock's policy, exhaustively.
//
// The lock is only as good as its weakest action: one `LiveAction` variant left
// out of the content list is a route to the projector that the lock does not
// cover. So the table below enumerates EVERY variant of `LiveAction` — if Rust
// grows an eighth one, `exhaustive` fails to compile before anyone ships a hole.
import { describe, expect, it } from "vitest";

import type { LiveAction } from "@/lib/bindings";
import {
  guardAction,
  guardGoLive,
  isContentAction,
  isEmergencyAction,
} from "./outputGuard";

/** Every action Rust accepts, with the verdict the lock must reach. */
const ACTIONS: Array<{ action: LiveAction; content: boolean }> = [
  { action: { type: "next" }, content: true },
  { action: { type: "previous" }, content: true },
  { action: { type: "go_to", index: 4 }, content: true },
  { action: { type: "show_logo" }, content: true },
  {
    action: { type: "show_message", text: "Barnevakt til rom 2" },
    content: true,
  },
  { action: { type: "blackout" }, content: false },
  { action: { type: "clear" }, content: false },
];

// A compile-time census: this fails to typecheck the day `LiveAction` gains a
// variant the table above does not mention.
type Covered = (typeof ACTIONS)[number]["action"]["type"];
type Missing = Exclude<LiveAction["type"], Covered>;
const exhaustive: Missing[] = [];

describe("the output lock covers every action Rust accepts", () => {
  it("mentions every LiveAction variant", () => {
    expect(exhaustive).toEqual([]);
    expect(new Set(ACTIONS.map((a) => a.action.type)).size).toBe(
      ACTIONS.length,
    );
  });

  for (const { action, content } of ACTIONS) {
    it(`${action.type} is ${content ? "content" : "an emergency stop"}`, () => {
      expect(isContentAction(action)).toBe(content);
      expect(isEmergencyAction(action)).toBe(!content);
    });
  }
});

describe("guardAction", () => {
  it("lets everything through when the output is unlocked", () => {
    for (const { action } of ACTIONS) {
      expect(guardAction(false, action), action.type).toBe("allow");
    }
  });

  it("blocks every content action when locked", () => {
    for (const { action, content } of ACTIONS.filter((a) => a.content)) {
      expect(content).toBe(true);
      expect(guardAction(true, action), action.type).toBe("blocked");
    }
  });

  it("never blocks blackout — the panic key outranks the lock", () => {
    expect(guardAction(true, { type: "blackout" })).toBe("allow");
  });

  it("never blocks clear — it only ever takes an override off", () => {
    expect(guardAction(true, { type: "clear" })).toBe("allow");
  });

  it("blocks the routes an operator actually presses", () => {
    // Space/Enter/Go and the Jump modal both land on go_to; the network remote
    // lands on next/previous; the message popover on show_message.
    expect(guardAction(true, { type: "go_to", index: 0 })).toBe("blocked");
    expect(guardAction(true, { type: "next" })).toBe("blocked");
    expect(guardAction(true, { type: "show_message", text: "hei" })).toBe(
      "blocked",
    );
  });
});

describe("guardGoLive", () => {
  it("blocks going live while locked — that is its own route to the screen", () => {
    expect(guardGoLive(true)).toBe("blocked");
  });
  it("allows going live when unlocked", () => {
    expect(guardGoLive(false)).toBe("allow");
  });
});
