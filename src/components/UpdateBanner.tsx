/**
 * Phase 13.2 — auto-update banner.
 *
 * On launch, checks this install's update ring (stable or beta — see Settings →
 * Advanced) for a newer signed build. If one exists, offers a one-click
 * download + relaunch. No-ops silently outside Tauri / offline / when the ring
 * has nothing promoted, so it never gets in the way.
 */
import { useEffect, useState } from "react";
import { Download, X } from "lucide-react";

import {
  checkForUpdate,
  installAndRelaunch,
  type UpdateInfo,
} from "@/lib/updater";
import { useT } from "@/lib/i18n";

export function UpdateBanner() {
  const t = useT();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    checkForUpdate()
      .then((u) => u && setUpdate(u))
      .catch(() => {});
  }, []);

  if (!update || dismissed) return null;

  return (
    <div className="fixed right-4 bottom-4 z-50 w-[min(92vw,420px)] rounded-xl border border-[var(--color-accent)]/40 bg-[var(--color-bg-elevated)] p-4 shadow-[var(--shadow-elevated)]">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold">
            {t("updateAvailable")}
            {update.version ? ` (${update.version})` : ""}
          </p>
          {/* HVA som er nytt — hentet fra manifestets `notes`, som siden
              `docs/release-notes/<tagg>.md` er en tekst et menneske har skrevet
              til operatøren og ikke en fast setning fra byggefila. Uten dette
              var feltet fylt, båret hele veien til frontenden og så aldri vist:
              v0.8.0-beta.1 flyttet blackout fra Escape til ⇧B, og banneret sa
              «Last ned og start på nytt».

              Ren tekst med bevarte linjeskift, ikke markdown — vakten i
              `scripts/release-notes.mjs` avviser markdown i notatet nettopp
              fordi denne boksen ikke har noen renderer å vise det med. */}
          {update.notes?.trim() ? (
            <p className="mt-2 max-h-48 overflow-y-auto text-xs whitespace-pre-line text-[var(--color-fg)]">
              {update.notes.trim()}
            </p>
          ) : (
            <p className="mt-1 text-xs text-[var(--color-fg-muted)]">
              {t("updateBody")}
            </p>
          )}
          {/* WHICH ring this build came from. An operator on beta who switches
              back to stable must be able to see that a banner still on screen
              is the beta offer — the install path refuses it, and this is the
              half of that rule they can read. */}
          <p className="mt-1 text-xs text-[var(--color-fg-muted)]">
            {t("updateFromRing", {
              ring:
                update.channel === "beta"
                  ? t("setChannelBeta")
                  : t("setChannelStable"),
            })}
          </p>
        </div>
        <button
          type="button"
          aria-label={t("actionClose")}
          onClick={() => setDismissed(true)}
          className="rounded-md p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <X size={16} />
        </button>
      </div>
      <div className="mt-3 flex justify-end">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            installAndRelaunch().catch(() => setBusy(false));
          }}
          className="flex items-center gap-2 rounded-md bg-[var(--color-accent)] px-4 py-1.5 text-sm font-bold text-[var(--color-accent-fg)] hover:brightness-110 disabled:opacity-60"
        >
          <Download size={14} />
          {busy ? t("updateInProgress") : t("updateDownload")}
        </button>
      </div>
    </div>
  );
}
