import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";

import { cn } from "@/lib/cn";

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  description?: string;
  children?: ReactNode;
  footer?: ReactNode;
  className?: string;
}

// Lightweight modal: backdrop + Escape-to-close + focus trap-lite (autoFocus on
// the panel). Good enough for confirmations and small forms; revisit if we need
// full ARIA dialog semantics with nested focus management.
export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  className,
}: DialogProps) {
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-50 grid place-items-center p-4">
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        className={cn(
          "relative w-full max-w-md overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]",
          className,
        )}
      >
        <button
          type="button"
          onClick={onClose}
          aria-label="Lukk"
          className="absolute top-3 right-3 rounded-md p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <X size={16} />
        </button>
        {(title || description) && (
          <div className="flex flex-col gap-1 p-5 pb-3">
            {title && (
              <h2 className="text-[var(--text-ui-lg)] font-semibold">
                {title}
              </h2>
            )}
            {description && (
              <p className="text-[var(--text-ui-sm)] text-[var(--color-fg-muted)]">
                {description}
              </p>
            )}
          </div>
        )}
        {children && <div className="px-5 pb-5">{children}</div>}
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t border-[var(--color-border)] p-4">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
