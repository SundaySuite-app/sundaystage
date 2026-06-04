/**
 * A tiny single-flight guard for an idempotent async action.
 *
 * Going live is the canonical case: the operator can fire `go()` faster than a
 * Rust round-trip completes (Space auto-repeat, a double-tap, Enter+Space).
 * `OperatorWorkspace.go()` reads the `session` React state to decide whether to
 * start a session, but that state only updates *after* `ipc.live.start()`
 * resolves — so two keypresses inside that window both see `session === null`
 * and both call `live_start`. The second call truncates the first's
 * crash-recovery WAL mid-write, jumps `started_at` (the stage timer visibly
 * resets), and re-zeroes the companion `seq`.
 *
 * `SingleFlight` collapses concurrent calls into one: while a run is in flight,
 * every caller awaits the *same* promise instead of launching a second one. It
 * is intentionally framework-free so it can be held in a `useRef` and unit
 * tested without rendering.
 */
export class SingleFlight<T> {
  private inFlight: Promise<T> | null = null;

  /**
   * Run `fn`, or—if a run is already in flight—return that run's promise
   * without invoking `fn` again. The slot is freed once the run settles, so a
   * later (non-overlapping) call starts a fresh run.
   */
  run(fn: () => Promise<T>): Promise<T> {
    if (this.inFlight) return this.inFlight;
    const p = fn().finally(() => {
      // Only clear if we're still the active run (defensive against reentry).
      if (this.inFlight === p) this.inFlight = null;
    });
    this.inFlight = p;
    return p;
  }

  /** Whether a run is currently in flight. */
  get busy(): boolean {
    return this.inFlight !== null;
  }
}
