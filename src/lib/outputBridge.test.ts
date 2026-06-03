// outputBridge — the operator → output signal bus.
//
// This hook backs the crash-isolation promise ("NEVER crash on a Sunday"): the
// output windows render whatever the operator broadcasts, and a frozen operator
// UI must be *observable* (heartbeats stop) so each output window's watchdog can
// hold the last frame. We test the emitter side purely with fake timers and a
// mocked Tauri event bus — no Tauri runtime, no network.
//
// Coverage: (1) OUTPUT_RENDER fires with a strictly-monotonic seq, and only when
// the session is active AND there is a frame; (2) HEARTBEAT emits every 250ms
// while active, stops on unmount and on active→false, and restarts on a fresh
// activation; (3) safeEmit swallows a rejected emit (the browser/non-Tauri case)
// without throwing.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";

import type { LiveFrame } from "@/lib/bindings";

// A capturing mock for Tauri's event bus. `emitImpl` is swapped per-test so we
// can make `emit` resolve (captured) or reject (non-Tauri).
const emitCalls: { event: string; payload?: unknown }[] = [];
let emitImpl: (event: string, payload?: unknown) => Promise<void> = async (
  event,
  payload,
) => {
  emitCalls.push({ event, payload });
};

vi.mock("@tauri-apps/api/event", () => ({
  emit: (event: string, payload?: unknown) => emitImpl(event, payload),
}));

// Imported after the mock is registered.
import {
  useOutputBridge,
  OUTPUT_RENDER,
  OUTPUT_HEARTBEAT,
} from "@/lib/outputBridge";

const FRAME: LiveFrame = { kind: "black" };
const HEARTBEAT_MS = 250;

const renders = () => emitCalls.filter((c) => c.event === OUTPUT_RENDER);
const heartbeats = () => emitCalls.filter((c) => c.event === OUTPUT_HEARTBEAT);

beforeEach(() => {
  emitCalls.length = 0;
  emitImpl = async (event, payload) => {
    emitCalls.push({ event, payload });
  };
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

// ───────────────────────── render broadcast ───────────────────────────────

describe("useOutputBridge — render broadcast", () => {
  it("does not broadcast a frame while inactive", () => {
    renderHook(({ f, a }) => useOutputBridge(f, a), {
      initialProps: { f: FRAME, a: false },
    });
    expect(renders()).toHaveLength(0);
  });

  it("does not broadcast when active but there is no frame", () => {
    renderHook(({ f, a }) => useOutputBridge(f, a), {
      initialProps: { f: null as LiveFrame | null, a: true },
    });
    expect(renders()).toHaveLength(0);
  });

  it("broadcasts the frame once it is active, with seq 1", () => {
    renderHook(({ f, a }) => useOutputBridge(f, a), {
      initialProps: { f: FRAME, a: true },
    });
    expect(renders()).toHaveLength(1);
    expect(renders()[0].payload).toEqual({ frame: FRAME, seq: 1 });
  });

  it("stamps a strictly-monotonic seq across frame changes", () => {
    const { rerender } = renderHook(({ f, a }) => useOutputBridge(f, a), {
      initialProps: { f: FRAME as LiveFrame | null, a: true },
    });
    rerender({ f: { kind: "logo" }, a: true });
    rerender({ f: { kind: "message", text: "Velkommen" }, a: true });

    const seqs = renders().map((c) => (c.payload as { seq: number }).seq);
    expect(seqs).toEqual([1, 2, 3]);
  });

  it("keeps the seq counter (does not reset) when toggled active→false→true", () => {
    const { rerender } = renderHook(({ f, a }) => useOutputBridge(f, a), {
      initialProps: { f: FRAME as LiveFrame | null, a: true },
    });
    rerender({ f: { kind: "logo" }, a: true }); // seq 2
    rerender({ f: { kind: "logo" }, a: false }); // inactive: no emit
    rerender({ f: { kind: "logo" }, a: true }); // active again: seq 3

    const seqs = renders().map((c) => (c.payload as { seq: number }).seq);
    expect(seqs).toEqual([1, 2, 3]);
  });
});

// ───────────────────────── heartbeat ──────────────────────────────────────

describe("useOutputBridge — heartbeat", () => {
  it("does not heartbeat while inactive", () => {
    renderHook(({ a }) => useOutputBridge(FRAME, a), {
      initialProps: { a: false },
    });
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS * 4));
    expect(heartbeats()).toHaveLength(0);
  });

  it("emits a heartbeat on the 250ms interval while active", () => {
    renderHook(() => useOutputBridge(FRAME, true));
    expect(heartbeats()).toHaveLength(0); // interval, not leading edge
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS));
    expect(heartbeats()).toHaveLength(1);
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS * 3));
    expect(heartbeats()).toHaveLength(4);
    expect(heartbeats()[0].payload).toMatchObject({ at: expect.any(Number) });
  });

  it("stops heart-beating on unmount (observable freeze)", () => {
    const { unmount } = renderHook(() => useOutputBridge(FRAME, true));
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS));
    expect(heartbeats()).toHaveLength(1);

    unmount();
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS * 5));
    // No further beats after teardown — the output watchdog will see the gap.
    expect(heartbeats()).toHaveLength(1);
  });

  it("clears the interval when active flips to false", () => {
    const { rerender } = renderHook(({ a }) => useOutputBridge(FRAME, a), {
      initialProps: { a: true },
    });
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS));
    expect(heartbeats()).toHaveLength(1);

    rerender({ a: false });
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS * 5));
    expect(heartbeats()).toHaveLength(1);

    // Re-activating starts a fresh interval.
    rerender({ a: true });
    act(() => vi.advanceTimersByTime(HEARTBEAT_MS));
    expect(heartbeats()).toHaveLength(2);
  });
});

// ───────────────────────── safeEmit resilience ────────────────────────────

describe("useOutputBridge — safeEmit swallows failures", () => {
  it("does not throw when emit rejects (non-Tauri / browser tests)", () => {
    emitImpl = async () => {
      throw new Error("not running in a Tauri context");
    };
    // Mounting (which emits a render) and ticking a heartbeat must not throw,
    // even though every emit rejects.
    expect(() => {
      const { unmount } = renderHook(() => useOutputBridge(FRAME, true));
      act(() => vi.advanceTimersByTime(HEARTBEAT_MS));
      unmount();
    }).not.toThrow();
  });
});
