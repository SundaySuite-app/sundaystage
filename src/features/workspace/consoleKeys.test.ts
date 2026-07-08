// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { keyScope } from "./consoleKeys";

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
      keyScope(el(`<div data-console-dock><button data-target>b</button></div>`)),
    ).toBe("dock");
    expect(
      keyScope(el(`<div data-console-dock><div><a data-target>x</a></div></div>`)),
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
