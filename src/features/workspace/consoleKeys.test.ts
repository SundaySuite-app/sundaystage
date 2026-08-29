// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import {
  consumesKey,
  keyScope,
  resolveConsoleKey,
  type ConsoleKeyContext,
  type KeyStroke,
} from "./consoleKeys";

function el(html: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = html;
  document.body.appendChild(host);
  return host.querySelector<HTMLElement>("[data-target]")!;
}

describe("keyScope", () => {
  it("classifies text entry — typing must never trigger transport", () => {
    expect(keyScope(el(`<input data-target />`))).toBe("text");
    expect(keyScope(el(`<textarea data-target></textarea>`))).toBe("text");
    expect(keyScope(el(`<select data-target></select>`))).toBe("text");
    expect(keyScope(el(`<div data-target contenteditable="true"></div>`))).toBe(
      "text",
    );
  });

  it("classifies focus inside the docked browser as dock", () => {
    expect(
      keyScope(
        el(`<div data-console-dock><button data-target>b</button></div>`),
      ),
    ).toBe("dock");
    expect(
      keyScope(
        el(`<div data-console-dock><div><a data-target>x</a></div></div>`),
      ),
    ).toBe("dock");
  });

  it("text entry wins over dock (search field inside the browser)", () => {
    expect(
      keyScope(el(`<div data-console-dock><input data-target /></div>`)),
    ).toBe("text");
  });

  it("everything else is console — including grid slide buttons", () => {
    expect(keyScope(el(`<button data-target>slide</button>`))).toBe("console");
    expect(keyScope(el(`<div data-target></div>`))).toBe("console");
    expect(keyScope(document.body)).toBe("console");
    expect(keyScope(null)).toBe("console");
  });
});

// ── resolveConsoleKey ────────────────────────────────────────────────────────
//
// This is the console's real key table, not a mirror of it: `OperatorWorkspace`
// calls exactly this function, so a change here that the console disagrees with
// cannot exist.

function stroke(key: string, mods: Partial<KeyStroke> = {}): KeyStroke {
  return { key, metaKey: false, ctrlKey: false, shiftKey: false, ...mods };
}

function ctx(over: Partial<ConsoleKeyContext> = {}): ConsoleKeyContext {
  return {
    scope: "console",
    isLive: true,
    browserOpen: false,
    modalOpen: false,
    ...over,
  };
}

describe("resolveConsoleKey — Escape is no longer a projector key", () => {
  it("Escape does nothing at all when no browser is open", () => {
    // The Sunday trap: Escape is the reflex for dismissing a dialog, and it
    // used to black the congregation screen.
    expect(resolveConsoleKey(stroke("Escape"), ctx())).toBe("none");
    expect(resolveConsoleKey(stroke("Escape"), ctx({ isLive: false }))).toBe(
      "none",
    );
  });

  it("Escape still closes the docked browser", () => {
    expect(
      resolveConsoleKey(stroke("Escape"), ctx({ browserOpen: true })),
    ).toBe("close-browser");
  });
});

describe("resolveConsoleKey — ⇧B is the blackout chord", () => {
  it("Shift+B blacks out while live", () => {
    expect(resolveConsoleKey(stroke("B", { shiftKey: true }), ctx())).toBe(
      "blackout",
    );
  });

  it("Shift+B with Caps Lock on (key reads 'b') still blacks out", () => {
    expect(resolveConsoleKey(stroke("b", { shiftKey: true }), ctx())).toBe(
      "blackout",
    );
  });

  it("a bare B does not black out — the chord is deliberate", () => {
    expect(resolveConsoleKey(stroke("b"), ctx())).toBe("none");
    expect(resolveConsoleKey(stroke("B"), ctx())).toBe("none");
  });

  it("does nothing when there is no live session to black out", () => {
    expect(
      resolveConsoleKey(
        stroke("B", { shiftKey: true }),
        ctx({ isLive: false }),
      ),
    ).toBe("none");
  });

  it("reaches the console even from inside the docked browser (panic key)", () => {
    expect(
      resolveConsoleKey(
        stroke("B", { shiftKey: true }),
        ctx({ scope: "dock" }),
      ),
    ).toBe("blackout");
  });
});

describe("resolveConsoleKey — the lock and the restore", () => {
  it("⌘L / Ctrl+L toggles the output lock, live or not", () => {
    expect(resolveConsoleKey(stroke("l", { metaKey: true }), ctx())).toBe(
      "toggle-lock",
    );
    expect(
      resolveConsoleKey(stroke("l", { ctrlKey: true }), ctx({ isLive: false })),
    ).toBe("toggle-lock");
  });

  it("a bare L is still the logo, not the lock", () => {
    expect(resolveConsoleKey(stroke("l"), ctx())).toBe("logo");
    expect(resolveConsoleKey(stroke("L"), ctx())).toBe("logo");
  });

  it("⌘Z / Ctrl+Z asks to restore what was cleared", () => {
    expect(resolveConsoleKey(stroke("z", { metaKey: true }), ctx())).toBe(
      "undo-clear",
    );
    expect(resolveConsoleKey(stroke("z", { ctrlKey: true }), ctx())).toBe(
      "undo-clear",
    );
  });

  it("both are swallowed so the browser does not act on them", () => {
    expect(consumesKey("toggle-lock")).toBe(true);
    expect(consumesKey("undo-clear")).toBe(true);
  });
});

describe("resolveConsoleKey — the transport is unchanged", () => {
  const cases: Array<[KeyStroke, string]> = [
    [stroke(" "), "go"],
    [stroke("Enter"), "go"],
    [stroke("g"), "go"],
    [stroke("G"), "go"],
    [stroke("ArrowRight"), "preview-next"],
    [stroke("ArrowDown"), "preview-next"],
    [stroke("PageDown"), "preview-next"],
    [stroke("ArrowLeft"), "preview-prev"],
    [stroke("ArrowUp"), "preview-prev"],
    [stroke("PageUp"), "preview-prev"],
    [stroke("Home"), "preview-first"],
    [stroke("End"), "preview-last"],
    [stroke("?"), "shortcuts"],
    [stroke("j", { metaKey: true }), "jump"],
    [stroke("b", { metaKey: true }), "toggle-browser"],
  ];
  for (const [s, expected] of cases) {
    it(`${s.metaKey ? "⌘" : ""}${s.key} → ${expected}`, () => {
      expect(resolveConsoleKey(s, ctx())).toBe(expected);
    });
  }

  it("leaves ⌘K to cmdk and every other chord to the system", () => {
    expect(resolveConsoleKey(stroke("k", { metaKey: true }), ctx())).toBe(
      "none",
    );
    expect(resolveConsoleKey(stroke("s", { metaKey: true }), ctx())).toBe(
      "none",
    );
  });

  it("⌘J needs a live session", () => {
    expect(
      resolveConsoleKey(stroke("j", { metaKey: true }), ctx({ isLive: false })),
    ).toBe("none");
  });
});

describe("resolveConsoleKey — scoping", () => {
  it("gives the console nothing at all while typing", () => {
    for (const s of [
      stroke(" "),
      stroke("Enter"),
      stroke("B", { shiftKey: true }),
      stroke("l", { metaKey: true }),
      stroke("z", { metaKey: true }),
      stroke("Escape"),
    ]) {
      expect(resolveConsoleKey(s, ctx({ scope: "text" })), s.key).toBe("none");
    }
  });

  it("gives the console nothing behind a modal", () => {
    for (const s of [
      stroke(" "),
      stroke("B", { shiftKey: true }),
      stroke("l", { metaKey: true }),
    ]) {
      expect(resolveConsoleKey(s, ctx({ modalOpen: true })), s.key).toBe(
        "none",
      );
    }
  });

  it("keeps Space and the arrows local to the docked browser", () => {
    expect(resolveConsoleKey(stroke(" "), ctx({ scope: "dock" }))).toBe("none");
    expect(resolveConsoleKey(stroke("ArrowDown"), ctx({ scope: "dock" }))).toBe(
      "none",
    );
    // …while the logo panic key still gets through.
    expect(resolveConsoleKey(stroke("l"), ctx({ scope: "dock" }))).toBe("logo");
  });
});
