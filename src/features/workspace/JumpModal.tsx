/**
 * ⌘J quick-jump — fuzzy-filter the cue list and jump straight on air. Mirrors
 * the old operator console's jump dialog.
 */
import { useMemo, useState } from "react";

import type { Cue } from "@/lib/bindings";
import { useT } from "@/lib/i18n";
import { cueDisplayLabel, cueId } from "./cueUtils";

export function JumpModal({
  cues,
  onPick,
  onClose,
}: {
  cues: Cue[];
  onPick: (index: number) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [q, setQ] = useState("");
  const matches = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return cues
      .map((cue, i) => ({ cue, i }))
      .filter(
        ({ cue }) =>
          !needle || cueDisplayLabel(cue, t).toLowerCase().includes(needle),
      )
      .slice(0, 50);
  }, [cues, q, t]);

  return (
    <div className="fixed inset-0 z-50 grid place-items-start pt-[14vh]">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative mx-auto w-full max-w-xl overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && matches[0]) onPick(matches[0].i);
            if (e.key === "Escape") onClose();
          }}
          placeholder={t("lpJumpPlaceholder")}
          className="w-full border-b border-[var(--color-border)] bg-transparent px-4 py-3 text-sm focus:outline-none"
        />
        <ul className="max-h-[50vh] overflow-y-auto p-2">
          {matches.map(({ cue, i }) => (
            <li key={cueId(cue)}>
              <button
                type="button"
                onClick={() => onPick(i)}
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-[var(--color-bg-surface)]"
              >
                <span className="w-8 font-mono text-[10px] text-[var(--color-fg-muted)]">
                  {i + 1}
                </span>
                <span className="flex-1 truncate">
                  {cueDisplayLabel(cue, t)}
                </span>
              </button>
            </li>
          ))}
          {matches.length === 0 && (
            <li className="px-3 py-6 text-center text-sm text-[var(--color-fg-muted)]">
              {t("liveNoMatches")}
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}
