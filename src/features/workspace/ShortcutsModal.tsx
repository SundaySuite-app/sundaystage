/**
 * Keyboard shortcuts reference modal — accessed via "?" or the ? button in
 * the TransportBar. Shows all global workspace shortcuts so operators can
 * learn the console without opening the manual.
 */
import { useEffect } from "react";
import { Keyboard, X } from "lucide-react";

import { useT } from "@/lib/i18n";

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

  const groups: Array<{
    heading: string;
    rows: Array<{ keys: string[]; action: string }>;
  }> = [
    {
      heading: t("kbGroupPlayback"),
      rows: [
        { keys: ["Space", "Enter", "G"], action: t("kbGo") },
        { keys: ["←", "↑"], action: t("kbPrev") },
        { keys: ["→", "↓"], action: t("kbNext") },
        { keys: ["Home"], action: t("kbFirst") },
        { keys: ["End"], action: t("kbLast") },
      ],
    },
    {
      heading: t("kbGroupOutput"),
      rows: [
        { keys: ["Esc", "B"], action: t("kbBlackout") },
        { keys: ["L"], action: t("kbLogo") },
      ],
    },
    {
      heading: t("kbGroupWorkspace"),
      rows: [
        { keys: ["⌘J"], action: t("kbJump") },
        { keys: ["?"], action: t("kbShortcutsHelp") },
      ],
    },
  ];

  return (
    <div className="fixed inset-0 z-50 grid place-items-center p-4">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative w-full max-w-md rounded-xl border border-[var(--color-border)] bg-[var(--color-bg)] shadow-[var(--shadow-elevated)]">
        <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-5 py-3">
          <Keyboard
            size={16}
            className="text-[var(--color-accent)]"
            aria-hidden
          />
          <h2 className="flex-1 text-sm font-semibold">{t("kbModalTitle")}</h2>
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
                        <div className="flex flex-wrap gap-1">
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
