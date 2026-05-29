/**
 * Phase 13.2 — auto-update banner.
 *
 * On launch, checks the GitHub Releases manifest for a newer signed build. If
 * one exists, offers a one-click download + relaunch. No-ops silently outside
 * Tauri / offline / before the first release, so it never gets in the way.
 */
import { useEffect, useState } from "react";
import { Download, X } from "lucide-react";

import { checkForUpdate, installAndRelaunch, type Update } from "@/lib/updater";
import { useT } from "@/lib/i18n";

export function UpdateBanner() {
  const t = useT();
  const [update, setUpdate] = useState<Update | null>(null);
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
          <p className="mt-1 text-xs text-[var(--color-fg-muted)]">
            {t("updateBody")}
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
            installAndRelaunch(update).catch(() => setBusy(false));
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
