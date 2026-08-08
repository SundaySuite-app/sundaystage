/**
 * E3 — the React error boundary around the operator workspace.
 *
 * `window.onerror` (see `lib/errorReporting.ts`) does NOT see an error thrown
 * during React's render: React catches it, unmounts the tree, and — with no
 * boundary — leaves a blank white screen with nothing in any log. On a Sunday
 * morning that is the worst possible failure, because the operator's only
 * evidence is "it went white".
 *
 * So this boundary does two things, in this order:
 *
 *   1. records the error into the same bounded, scrubbed crash ring the Rust
 *      panic hook writes to, as `webview_error`;
 *   2. shows the operator a plain recovery card instead of a blank page.
 *
 * **The live output is unaffected either way.** It runs in a separate OS
 * process holding the last frame (see `output::process`), which is the whole
 * reason this can be a calm card rather than an emergency.
 */

import { Component, type ErrorInfo, type ReactNode } from "react";

import { reportError } from "@/lib/errorReporting";
import { translate, useLocale, type TKey } from "@/lib/i18n";

interface Props {
  children: ReactNode;
  /** Name recorded with the crash record — which part of the UI failed. */
  component?: string;
}

interface State {
  failed: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // React's own component stack is the useful part — it names the component
    // that threw, which no JS stack frame does. It is developer-authored
    // component names only, and it goes through the same scrubbing and the
    // same 200-character cap as everything else in the ring.
    reportError("webview_error", error, {
      component: this.props.component ?? "OperatorWorkspace",
      location: info.componentStack?.trim().split("\n")[0]?.trim() ?? null,
    });
  }

  render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    return <ErrorFallback onReload={() => window.location.reload()} />;
  }
}

/**
 * Translate WITHOUT a hook.
 *
 * `useT()` is a Zustand hook, and this card renders inside a tree that has just
 * proved it can throw — subscribing to a store here is exactly the extra
 * machinery that turns a recoverable panel error into a hard blank screen. The
 * locale store exposes a plain `getState()`, and `translate` is pure, so the
 * card is fully localised with no subscription at all. If even that fails, the
 * English catalog entry is used directly.
 */
function safeT(key: TKey): string {
  try {
    return translate(useLocale.getState().lang, key);
  } catch {
    return translate("en", key);
  }
}

/**
 * The recovery card. Deliberately dependency-light: no hooks, no query client,
 * no icons — any of which could be the thing that broke.
 */
function ErrorFallback({ onReload }: { onReload: () => void }) {
  return (
    <div
      role="alert"
      className="grid h-screen w-screen place-items-center bg-[var(--color-bg)] p-8 text-[var(--color-fg)]"
    >
      <div className="flex max-w-md flex-col items-center gap-4 text-center">
        <div className="grid h-14 w-14 place-items-center rounded-2xl bg-[var(--color-brand)] text-2xl font-bold text-[var(--color-accent)]">
          S
        </div>
        <h1 className="text-[var(--text-ui-xl)] font-bold">
          {safeT("errBoundaryTitle")}
        </h1>
        <p className="text-sm text-[var(--color-fg-muted)]">
          {safeT("errBoundaryBody")}
        </p>
        <button
          type="button"
          onClick={onReload}
          className="rounded-lg bg-[var(--color-brand)] px-4 py-2 text-sm font-semibold text-[var(--color-accent)]"
        >
          {safeT("errBoundaryReload")}
        </button>
      </div>
    </div>
  );
}
