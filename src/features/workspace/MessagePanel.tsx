/**
 * Operator messages — "Barnevakt til rom 2", "Gudstjenesten starter om
 * 5 minutter" — pushed over the output without losing the cue position
 * (an `OutputState::Message` override, exactly like Blackout/Logo).
 *
 * An anchored popover, NOT a modal: it must never join the modal hotkey
 * guard, and the console stays fully operable while it is open. The subtree
 * is marked `data-console-dock` so typing here never triggers transport keys
 * while B/L still work (see consoleKeys.ts).
 */
import { useEffect, useRef, useState } from "react";
import { Send, X } from "lucide-react";

import { useT } from "@/lib/i18n";
import { cn } from "@/lib/cn";

const LAST_MESSAGE_KEY = "ss-last-operator-message";

interface Props {
  open: boolean;
  /** True while the output is showing a message. */
  active: boolean;
  onShow: (text: string) => void;
  onClear: () => void;
  onClose: () => void;
}

export function MessagePanel({ open, active, onShow, onClear, onClose }: Props) {
  const t = useT();
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Restore the last custom message — repeated announcements are the norm.
  useEffect(() => {
    if (!open) return;
    try {
      const last = localStorage.getItem(LAST_MESSAGE_KEY);
      if (last) setText(last);
    } catch {
      /* storage unavailable — start blank */
    }
    inputRef.current?.focus();
  }, [open]);

  if (!open) return null;

  const presets = [t("msgPresetStartingSoon"), t("msgPresetNursery")];

  function show(message: string) {
    const trimmed = message.trim();
    if (!trimmed) return;
    onShow(trimmed);
    try {
      localStorage.setItem(LAST_MESSAGE_KEY, trimmed);
    } catch {
      /* best-effort */
    }
  }

  return (
    <div
      data-console-dock
      role="dialog"
      aria-label={t("msgPanelTitle")}
      className="fixed top-14 left-1/2 z-40 w-[min(92vw,440px)] -translate-x-1/2 rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3 shadow-[var(--shadow-elevated)]"
    >
      <div className="mb-2 flex items-center">
        <h2 className="flex-1 text-xs font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
          {t("msgPanelTitle")}
        </h2>
        <button
          type="button"
          onClick={onClose}
          title={t("actionClose")}
          className="rounded-md p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <X size={14} />
        </button>
      </div>

      <div className="flex flex-col gap-1.5">
        {presets.map((p) => (
          <button
            key={p}
            type="button"
            onClick={() => show(p)}
            className="rounded-md border border-[var(--color-border)] px-3 py-1.5 text-left text-sm hover:border-[var(--color-accent)] hover:bg-[var(--color-bg-surface)]"
          >
            {p}
          </button>
        ))}
      </div>

      <div className="mt-2 flex gap-1.5">
        <input
          ref={inputRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") show(text);
            if (e.key === "Escape") onClose();
          }}
          placeholder={t("msgPlaceholder")}
          className="min-w-0 flex-1 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2.5 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
        />
        <button
          type="button"
          onClick={() => show(text)}
          disabled={text.trim() === ""}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-xs font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-40"
        >
          <Send size={12} aria-hidden />
          {t("msgShow")}
        </button>
      </div>

      <button
        type="button"
        onClick={onClear}
        disabled={!active}
        className={cn(
          "mt-2 w-full rounded-md border px-3 py-1.5 text-xs font-medium",
          active
            ? "border-[var(--color-accent)] text-[var(--color-accent)] hover:bg-[var(--color-bg-surface)]"
            : "border-[var(--color-border)] text-[var(--color-fg-muted)] opacity-50",
        )}
      >
        {t("msgClear")}
      </button>
    </div>
  );
}
