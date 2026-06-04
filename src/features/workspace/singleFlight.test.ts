import { describe, it, expect, vi } from "vitest";

import { SingleFlight } from "./singleFlight";

/** A promise plus its resolver, so a test can control when an async run settles. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("SingleFlight (go-live double-fire guard)", () => {
  it("collapses concurrent calls into a single run", async () => {
    const sf = new SingleFlight<number>();
    const d = deferred<number>();
    const fn = vi.fn(() => d.promise);

    // Repro: the operator double-taps Space before the first live_start resolves.
    const p1 = sf.run(fn);
    const p2 = sf.run(fn);

    // The underlying action (ipc.live.start) must fire exactly once — the
    // second tap must NOT issue a second live_start that truncates the WAL /
    // resets started_at + companion seq.
    expect(fn).toHaveBeenCalledTimes(1);
    expect(p2).toBe(p1); // both callers await the same in-flight promise

    d.resolve(7);
    await expect(p1).resolves.toBe(7);
    await expect(p2).resolves.toBe(7);
  });

  it("allows a fresh run once the previous one settles", async () => {
    const sf = new SingleFlight<string>();

    const first = await sf.run(() => Promise.resolve("a"));
    expect(first).toBe("a");
    expect(sf.busy).toBe(false);

    const second = await sf.run(() => Promise.resolve("b"));
    expect(second).toBe("b");
  });

  it("frees the slot after a rejection so the next call can retry", async () => {
    const sf = new SingleFlight<string>();

    await expect(
      sf.run(() => Promise.reject(new Error("boom"))),
    ).rejects.toThrow("boom");
    expect(sf.busy).toBe(false);

    // A retry must actually run (not be swallowed by a stuck in-flight slot).
    const fn = vi.fn(() => Promise.resolve("ok"));
    await expect(sf.run(fn)).resolves.toBe("ok");
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("reports busy while a run is pending", async () => {
    const sf = new SingleFlight<void>();
    const d = deferred<void>();
    const p = sf.run(() => d.promise);
    expect(sf.busy).toBe(true);
    d.resolve();
    await p;
    expect(sf.busy).toBe(false);
  });
});
