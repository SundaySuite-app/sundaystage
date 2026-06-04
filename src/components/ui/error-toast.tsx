import { X } from "lucide-react";

/**
 * ErrorToast — a small, dismissable banner for surfacing failures that would
 * otherwise be swallowed (e.g. a failed disk write on a settings save).
 *
 * Live output is sacrosanct, so these never block or throw — they just make a
 * silent failure visible. The companion `useErrorToast` hook (src/lib) owns the
 * message and an auto-dismiss timer; render `<ErrorToast … />` once near the
 * root of a panel. The caller passes a localized message, so this primitive
 * carries no i18n dependency.
 */
export interface ErrorToastProps {
  message: string | null;
  onDismiss: () => void;
}

export function ErrorToast({ message, onDismiss }: ErrorToastProps) {
  if (!message) return null;
  return (
    <div
      role="alert"
      className="fixed bottom-4 left-1/2 z-[60] flex max-w-[90vw] -translate-x-1/2 items-center gap-3 rounded-lg border border-[var(--color-danger)]/50 bg-[var(--color-bg-elevated)] px-4 py-2 text-sm text-[var(--color-fg)] shadow-[var(--shadow-elevated)]"
    >
      <span className="h-2 w-2 shrink-0 rounded-full bg-[var(--color-danger)]" />
      <span>{message}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]"
        aria-label="Dismiss"
      >
        <X size={14} />
      </button>
    </div>
  );
}
