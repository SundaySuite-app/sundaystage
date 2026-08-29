/**
 * "Restore what you just cleared" — the undo offer after Clear.
 *
 * A short-lived bar with a live countdown, because an offer that does not say
 * how long it stands is an offer the operator has to gamble on. It carries a
 * real button as well as the chord: the keyboard is the fast path, but a
 * volunteer who has just made a mistake in front of a congregation reaches for
 * the mouse.
 *
 * `aria-live="polite"` and not `role="alert"`: this is a chance, not a failure,
 * and it must never interrupt a screen reader mid-sentence during a service.
 */
import { useEffect, useState } from "react";
import { Undo2 } from "lucide-react";

import { useT } from "@/lib/i18n";
import { modChord } from "@/lib/platform";
import { secondsLeft } from "./clearUndo";

interface Props {
  /** Already-localized sentence naming what was cleared. */
  message: string;
  /** `Date.now()` when the clear happened. */
  startedAt: number;
  onRestore: () => void;
}

export function UndoToast({ message, startedAt, onRestore }: Props) {
  const t = useT();
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(id);
  }, [startedAt]);

  const remaining = secondsLeft(startedAt, now);
  const chord = modChord("Z");

  return (
    <div
      aria-live="polite"
      className="fixed bottom-4 left-1/2 z-[55] flex max-w-[90vw] -translate-x-1/2 items-center gap-3 rounded-lg border border-[var(--color-accent)]/40 bg-[var(--color-bg-elevated)] px-4 py-2 text-sm text-[var(--color-fg)] shadow-[var(--shadow-elevated)]"
    >
      <span>{message}</span>
      <button
        type="button"
        onClick={onRestore}
        className="flex items-center gap-1.5 rounded-md bg-[var(--color-accent)] px-2.5 py-1 text-xs font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110"
      >
        <Undo2 size={12} aria-hidden />
        {t("undoRestore")}
      </button>
      <span className="font-mono text-xs tabular-nums text-[var(--color-fg-muted)]">
        {t("undoCountdown", { key: chord, seconds: remaining })}
      </span>
    </div>
  );
}
