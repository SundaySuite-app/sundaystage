/**
 * Post-service export — Phase 10.2.
 *
 * The marquee Sunday-suite demo: from the live session log alone, generate
 * recording chapter markers and an SRT caption file that line up with a
 * SundayRec recording — the operator did nothing extra. File-system save and
 * the live hand-off to SundayRec ride the bridge (Phase 10.1, deferred); here
 * the operator can preview and copy.
 */

import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Clapperboard, Copy, X } from "lucide-react";

import type { ChapterMarker } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";

function fmtOffset(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
}

export function ExportModal({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [tab, setTab] = useState<"chapters" | "srt">("chapters");
  const [copied, setCopied] = useState(false);

  const markersQuery = useQuery({
    queryKey: ["chapterMarkers"],
    queryFn: () => ipc.live.chapterMarkers(),
  });
  const srtQuery = useQuery({
    queryKey: ["exportSrt"],
    queryFn: () => ipc.live.exportSrt(),
  });

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const markers = markersQuery.data ?? [];
  const srt = srtQuery.data ?? "";

  const copy = async () => {
    const text =
      tab === "srt"
        ? srt
        : markers
            .map((m) => `${fmtOffset(Number(m.offset_ms))}  ${m.title}`)
            .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard may be unavailable; ignore */
    }
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center p-6">
      <div
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <header className="flex items-center gap-2 border-b border-[var(--color-border)] px-4 py-3">
          <Clapperboard size={16} className="text-[var(--color-accent)]" />
          <h2 className="font-semibold">{t("exTitle")}</h2>
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            className="grid h-7 w-7 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={15} />
          </button>
        </header>

        <div className="flex items-center gap-1.5 border-b border-[var(--color-border)] px-4 py-2">
          {(["chapters", "srt"] as const).map((tabId) => (
            <button
              key={tabId}
              type="button"
              onClick={() => setTab(tabId)}
              className={cn(
                "rounded-md px-3 py-1 text-xs transition-colors",
                tab === tabId
                  ? "bg-[var(--color-accent)] text-[var(--color-sunday-blue-900)]"
                  : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
              )}
            >
              {tabId === "chapters" ? t("exTabChapters") : t("exTabSrt")}
            </button>
          ))}
          <div className="flex-1" />
          <button
            type="button"
            onClick={copy}
            className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <Copy size={13} /> {copied ? t("exCopied") : t("actionCopy")}
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4">
          {tab === "chapters" ? (
            <ChapterList markers={markers} loading={markersQuery.isLoading} />
          ) : (
            <pre className="whitespace-pre-wrap rounded-md bg-[var(--color-bg)] p-3 font-mono text-xs text-[var(--color-fg-muted)]">
              {srtQuery.isLoading
                ? t("exGenerating")
                : srt.trim() || t("exNoSlidesShown")}
            </pre>
          )}
        </div>

        <footer className="border-t border-[var(--color-border)] px-4 py-2 text-[11px] text-[var(--color-fg-muted)]">
          {t("exFooterNote")}
        </footer>
      </div>
    </div>
  );
}

function ChapterList({
  markers,
  loading,
}: {
  markers: ChapterMarker[];
  loading: boolean;
}) {
  const t = useT();
  if (loading)
    return (
      <p className="text-sm text-[var(--color-fg-muted)]">{t("exComputing")}</p>
    );
  if (markers.length === 0) {
    return (
      <p className="text-sm text-[var(--color-fg-muted)]">
        {t("exNoChapters")}
      </p>
    );
  }
  return (
    <ul className="space-y-1">
      {markers.map((m, i) => (
        <li
          key={i}
          className="flex items-center gap-3 rounded-md border border-[var(--color-border)] px-3 py-2 text-sm"
        >
          <span className="font-mono text-xs tabular-nums text-[var(--color-accent)]">
            {fmtOffset(Number(m.offset_ms))}
          </span>
          <span>{m.title}</span>
        </li>
      ))}
    </ul>
  );
}
