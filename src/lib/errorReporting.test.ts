/**
 * E3 — the renderer's crash capture, pinned.
 *
 * Two properties matter more than the plumbing:
 *
 *   * it must NEVER throw — this is the last handler in the chain, and an error
 *     raised inside an error handler is either a loop or a silent hole;
 *   * it must not amplify — a render loop throwing on every frame would fill
 *     the twenty-record ring in a second and evict the first, most diagnostic
 *     occurrence.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

const report = vi.fn((_entry: unknown) => Promise.resolve());

vi.mock("@/lib/ipc", () => ({
  ipc: { crash: { report: (entry: unknown) => report(entry) } },
}));

import {
  DEDUPE_WINDOW_MS,
  __resetForTests,
  describe as describeValue,
  firstFrame,
  installErrorReporting,
  reportError,
} from "@/lib/errorReporting";

let clock = 0;

beforeEach(() => {
  report.mockClear();
  report.mockImplementation(() => Promise.resolve());
  clock = 1_000;
  __resetForTests(() => clock);
});

describe("describe()", () => {
  it("takes an Error's first stack line, which names the error and where", () => {
    const e = new Error("boom");
    e.stack = "Error: boom\n    at foo (/Users/ola/app.js:1:2)";
    expect(describeValue(e)).toBe("Error: boom");
  });

  it("handles everything JavaScript can actually throw", () => {
    expect(describeValue("a string")).toBe("a string");
    expect(describeValue({ code: 7 })).toBe('{"code":7}');
    expect(describeValue(null)).toBe("null");
    // A circular object must not throw inside the error handler.
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(() => describeValue(circular)).not.toThrow();
    expect(describeValue(circular)).toContain("Object");
  });

  it("caps the message before it crosses IPC", () => {
    expect(describeValue("æ".repeat(5_000)).length).toBe(400);
  });
});

describe("firstFrame()", () => {
  it("finds the file:line:col of the frame that threw", () => {
    const e = new Error("boom");
    e.stack = [
      "Error: boom",
      "    at render (http://localhost:1420/src/App.tsx:42:7)",
      "    at other (http://localhost:1420/src/x.tsx:1:1)",
    ].join("\n");
    expect(firstFrame(e)).toBe("http://localhost:1420/src/App.tsx:42:7");
  });

  it("is null for anything without a usable stack", () => {
    expect(firstFrame("a string")).toBeNull();
    expect(firstFrame(new Error("no stack at all"))).toBeDefined();
    const bare = new Error("bare");
    bare.stack = "Error: bare";
    expect(firstFrame(bare)).toBeNull();
  });
});

describe("reportError()", () => {
  it("sends the kind, message and location into the crash ring", () => {
    const e = new Error("TypeError: cue is undefined");
    e.stack = [
      "Error: TypeError: cue is undefined",
      "    at go (/Users/ola/app/main.js:9:3)",
    ].join("\n");
    expect(reportError("webview_error", e, { component: "Workspace" })).toBe(
      true,
    );
    expect(report).toHaveBeenCalledWith({
      kind: "webview_error",
      message: "Error: TypeError: cue is undefined",
      // Scrubbing is Rust's job — the path is expected to travel this far.
      location: "/Users/ola/app/main.js:9:3",
      component: "Workspace",
    });
  });

  it("collapses a burst of the same message into one record", () => {
    // The failure this exists for: a render loop throwing every frame would
    // otherwise evict the first occurrence — the diagnostic one — from a ring
    // that only holds twenty.
    for (let i = 0; i < 200; i++) {
      clock += 10;
      reportError("webview_error", "the same failure over and over");
    }
    expect(report).toHaveBeenCalledTimes(1);

    // Past the window, it is news again.
    clock += DEDUPE_WINDOW_MS;
    expect(reportError("webview_error", "the same failure over and over")).toBe(
      true,
    );
    expect(report).toHaveBeenCalledTimes(2);
  });

  it("does not collapse DIFFERENT messages", () => {
    reportError("webview_error", "first problem");
    reportError("webview_error", "second problem");
    expect(report).toHaveBeenCalledTimes(2);
  });

  it("never throws, whatever the IPC layer does", () => {
    // No Tauri in a browser: `invoke` rejects, and a rejection here would
    // surface as an unhandled rejection — which this very module reports,
    // which would report again. The promise is swallowed on purpose.
    report.mockImplementation(() => Promise.reject(new Error("no tauri")));
    expect(() => reportError("webview_error", "offline")).not.toThrow();
    // …and a synchronous throw is contained too.
    report.mockImplementation(() => {
      throw new Error("invoke exploded");
    });
    expect(() => reportError("unhandled_rejection", "sync boom")).not.toThrow();
    expect(reportError("unhandled_rejection", "sync boom 2")).toBe(false);
  });

  it("ignores an empty message rather than writing a blank record", () => {
    expect(reportError("webview_error", "")).toBe(false);
    expect(report).not.toHaveBeenCalled();
  });
});

describe("installErrorReporting()", () => {
  it("captures window errors and unhandled rejections", () => {
    installErrorReporting();
    window.dispatchEvent(
      new ErrorEvent("error", {
        message: "ReferenceError: cueList is not defined",
        filename: "http://localhost:1420/src/live.tsx",
        lineno: 12,
        colno: 5,
      }),
    );
    expect(report).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "webview_error",
        message: "ReferenceError: cueList is not defined",
        location: "http://localhost:1420/src/live.tsx:12:5",
      }),
    );

    report.mockClear();
    // jsdom does not construct PromiseRejectionEvent, so dispatch the shape the
    // listener reads.
    const event = new Event("unhandledrejection") as Event & {
      reason?: unknown;
    };
    event.reason = "the publish promise rejected";
    window.dispatchEvent(event);
    expect(report).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: "unhandled_rejection",
        message: "the publish promise rejected",
      }),
    );
  });

  it("is idempotent, so StrictMode's double effect does not double every record", () => {
    installErrorReporting();
    installErrorReporting();
    installErrorReporting();
    window.dispatchEvent(new ErrorEvent("error", { message: "once please" }));
    expect(report).toHaveBeenCalledTimes(1);
  });
});
