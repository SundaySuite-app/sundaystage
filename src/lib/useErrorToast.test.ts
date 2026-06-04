/**
 * useErrorToast tests (headless-2).
 *
 * The save-failure banner is the only signal an operator gets when a disk write
 * fails, so its show/auto-dismiss/manual-dismiss behaviour is worth pinning.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import { useErrorToast } from "./useErrorToast";

describe("useErrorToast", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("starts empty", () => {
    const { result } = renderHook(() => useErrorToast());
    expect(result.current.message).toBeNull();
  });

  it("shows a message then auto-dismisses after the timeout", () => {
    const { result } = renderHook(() => useErrorToast(1000));
    act(() => result.current.showError("boom"));
    expect(result.current.message).toBe("boom");

    act(() => vi.advanceTimersByTime(999));
    expect(result.current.message).toBe("boom");

    act(() => vi.advanceTimersByTime(1));
    expect(result.current.message).toBeNull();
  });

  it("dismiss() clears immediately and cancels the auto-dismiss", () => {
    const { result } = renderHook(() => useErrorToast(1000));
    act(() => result.current.showError("boom"));
    act(() => result.current.dismiss());
    expect(result.current.message).toBeNull();

    // The pending timer must not resurrect or null-fire later.
    act(() => vi.advanceTimersByTime(2000));
    expect(result.current.message).toBeNull();
  });

  it("a second showError resets the auto-dismiss window", () => {
    const { result } = renderHook(() => useErrorToast(1000));
    act(() => result.current.showError("first"));
    act(() => vi.advanceTimersByTime(800));
    act(() => result.current.showError("second"));
    expect(result.current.message).toBe("second");

    // 800ms after the *second* call the first timer would have fired; it must
    // have been cleared, so the message is still visible.
    act(() => vi.advanceTimersByTime(800));
    expect(result.current.message).toBe("second");

    act(() => vi.advanceTimersByTime(200));
    expect(result.current.message).toBeNull();
  });
});
