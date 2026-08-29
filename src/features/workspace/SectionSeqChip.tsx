/**
 * The section sequence, on screen while it is being typed.
 *
 * Typing blind over a live congregation screen is frightening, and rightly so.
 * So the sequence is never invisible: every keystroke shows up here with the
 * section it currently points at, whether it can still change, and how to get
 * out (Esc). A sequence that matches nothing says so instead of failing
 * silently — "nothing happened" is the one answer an operator mid-service
 * cannot act on.
 *
 * `aria-live="polite"`, never `role="alert"`: this is the operator's own typing
 * echoed back, not a problem, and it must not cut across a screen reader during
 * a service.
 */
import { CornerDownLeft } from "lucide-react";

import { useT } from "@/lib/i18n";
import { cn } from "@/lib/cn";

interface Props {
  /** What has been typed so far ("v2"). */
  buffer: string;
  /** The section it points at right now, already localized. */
  label: string | null;
  /** A further keystroke could still land somewhere else. */
  ambiguous: boolean;
}

export function SectionSeqChip({ buffer, label, ambiguous }: Props) {
  const t = useT();
  return (
    <div
      aria-live="polite"
      data-testid="section-seq"
      className="pointer-events-none fixed bottom-20 left-1/2 z-[60] flex max-w-[90vw] -translate-x-1/2 items-center gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-3 py-2 shadow-[var(--shadow-elevated)]"
    >
      <kbd className="rounded border border-[var(--color-accent)]/50 bg-[var(--color-bg-surface)] px-2 py-0.5 font-mono text-sm font-bold tracking-[0.15em] text-[var(--color-accent)] uppercase">
        {buffer}
      </kbd>
      <span
        className={cn(
          "truncate text-sm",
          label ? "text-[var(--color-fg)]" : "text-[var(--color-fg-muted)]",
        )}
      >
        {label ?? t("secJumpNoMatch")}
      </span>
      {label && ambiguous && (
        <span className="flex items-center gap-1 text-xs text-[var(--color-fg-muted)]">
          <CornerDownLeft size={12} aria-hidden />
          {t("secJumpConfirm")}
        </span>
      )}
      <span className="text-xs text-[var(--color-fg-muted)]">
        {t("secJumpCancel")}
      </span>
    </div>
  );
}
