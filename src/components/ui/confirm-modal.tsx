import { Button } from "./button";

/**
 * ConfirmModal — a small, focused confirmation dialog for destructive actions.
 *
 * Extracted from the services TemplatesPage so theme/template/anything-delete
 * flows share one accessible, locale-aware modal instead of the browser's
 * blocking `window.confirm` (which can't be styled and reads in the OS locale).
 *
 * The caller owns its own visibility state and passes localized labels, so this
 * primitive stays free of any i18n dependency.
 */
export interface ConfirmModalProps {
  title: string;
  body: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmModal({
  title,
  body,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onCancel}
        aria-hidden
      />
      <div className="relative w-[min(90vw,420px)] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-[var(--shadow-elevated)]">
        <h2 className="font-semibold">{title}</h2>
        <p className="mt-1 text-sm text-[var(--color-fg-muted)]">{body}</p>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button variant="danger" onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
