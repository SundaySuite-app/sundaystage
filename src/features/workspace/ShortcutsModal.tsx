/**
 * Keyboard shortcuts reference modal — accessed via "?" or the ? button in
 * the TransportBar. Shows all global workspace shortcuts so operators can
 * learn the console without opening the manual.
 *
 * **Every row here is generated from `CONSOLE_SHORTCUTS` in `consoleKeys.ts`.**
 * The previous version of this file held a hand-typed copy of the key table,
 * and the copy was wrong: a printed shortcut that is wrong on half the installs
 * teaches the volunteer that the whole list can be ignored. The keycaps are now
 * formatted from the literal keystroke objects the console's own tests replay
 * through the resolver, so a row cannot say one thing while the console does
 * another. Platform chords (⌘L on a Mac, Ctrl+L on Windows) come out of the
 * same formatter.
 */
import { useEffect } from "react";
import { Keyboard, X } from "lucide-react";

import { useT } from "@/lib/i18n";
import { CONSOLE_SHORTCUTS, keyCap } from "./consoleKeys";

interface Props {
  onClose: () => void;
}

export function ShortcutsModal({ onClose }: Props) {
  const t = useT();

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const groups = CONSOLE_SHORTCUTS.map((g) => ({
    heading: t(g.heading),
    rows: g.rows.map((r) => ({
      keys: r.strokes.map((s) => keyCap(s)),
      // Typed one after another (`V` then `2`) rather than alternatives.
      typed: !!r.typed,
      action: t(r.label),
    })),
  }));

  return (
    <div className="fixed inset-0 z-50 grid place-items-center p-4">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcuts-modal-title"
        className="relative w-full max-w-md rounded-xl border border-[var(--color-border)] bg-[var(--color-bg)] shadow-[var(--shadow-elevated)]"
      >
        <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-5 py-3">
          <Keyboard
            size={16}
            className="text-[var(--color-accent)]"
            aria-hidden
          />
          <h2
            id="shortcuts-modal-title"
            className="flex-1 text-sm font-semibold"
          >
            {t("kbModalTitle")}
          </h2>
          <button
            type="button"
            onClick={onClose}
            title={t("actionClose")}
            className="rounded-md p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={15} />
          </button>
        </div>
        <div className="space-y-5 p-5">
          {groups.map((g) => (
            <section key={g.heading}>
              <h3 className="mb-2 text-[10px] font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
                {g.heading}
              </h3>
              <table className="w-full">
                <tbody>
                  {g.rows.map((row) => (
                    <tr key={row.action} className="group">
                      <td className="pb-1.5 pr-4 align-top">
                        <div
                          className={
                            row.typed
                              ? "flex flex-wrap gap-0.5"
                              : "flex flex-wrap gap-1"
                          }
                        >
                          {row.keys.map((k) => (
                            <kbd
                              key={k}
                              className="inline-flex items-center rounded border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-1.5 py-0.5 font-mono text-[11px] text-[var(--color-fg)]"
                            >
                              {k}
                            </kbd>
                          ))}
                        </div>
                      </td>
                      <td className="pb-1.5 text-sm text-[var(--color-fg-muted)]">
                        {row.action}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
