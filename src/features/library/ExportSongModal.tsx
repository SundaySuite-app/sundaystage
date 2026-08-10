/**
 * ExportSongModal — Spor B5 song export (the lock-in fix).
 *
 * Serialises a song to an open interchange format so a church can take its
 * library elsewhere: OpenLyrics 0.9 XML (read by OpenLP, ProPresenter, …) or a
 * ChordPro lead sheet. The `export_song` command returns the string; this modal
 * previews it and lets the operator copy it or save it to a file — mirroring
 * how the post-service `ExportModal` surfaces the SRT/manifest strings.
 */

import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Copy, Download, FileDown, X } from "lucide-react";

import { ipc } from "@/lib/ipc";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";

type ExportFormat = "openlyrics" | "chordpro";

const FORMAT_META: Record<ExportFormat, { label: string; ext: string }> = {
  openlyrics: { label: "OpenLyrics", ext: "xml" },
  chordpro: { label: "ChordPro", ext: "cho" },
};

/** Filesystem-safe basename from a song title. */
function safeName(title: string): string {
  const cleaned = title
    .trim()
    .replace(/[^\p{L}\p{N} _-]/gu, "")
    .trim();
  return cleaned.length > 0 ? cleaned : "song";
}

export function ExportSongModal({
  songId,
  songTitle,
  onClose,
}: {
  songId: string;
  songTitle: string;
  onClose: () => void;
}) {
  const t = useT();
  const [format, setFormat] = useState<ExportFormat>("openlyrics");
  const [copied, setCopied] = useState(false);

  const exportQuery = useQuery({
    queryKey: ["exportSong", songId, format],
    queryFn: () => ipc.song.export(songId, format),
  });

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const content = exportQuery.data ?? "";

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard may be unavailable; ignore */
    }
  };

  const download = () => {
    const { ext } = FORMAT_META[format];
    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${safeName(songTitle)}.${ext}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
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
          <FileDown size={16} className="text-[var(--color-accent)]" />
          <h2 className="font-semibold">{t("exportSongTitle")}</h2>
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            aria-label={t("actionClose")}
            className="grid h-7 w-7 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={15} />
          </button>
        </header>

        <div className="flex items-center gap-1.5 border-b border-[var(--color-border)] px-4 py-2">
          {(Object.keys(FORMAT_META) as ExportFormat[]).map((fmt) => (
            <button
              key={fmt}
              type="button"
              onClick={() => setFormat(fmt)}
              className={cn(
                "rounded-md px-3 py-1 text-xs transition-colors",
                format === fmt
                  ? "bg-[var(--color-accent)] text-[var(--color-sunday-blue-900)]"
                  : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
              )}
            >
              {FORMAT_META[fmt].label}
            </button>
          ))}
          <div className="flex-1" />
          <button
            type="button"
            onClick={copy}
            disabled={content.length === 0}
            className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
          >
            <Copy size={13} /> {copied ? t("exCopied") : t("actionCopy")}
          </button>
          <button
            type="button"
            onClick={download}
            disabled={content.length === 0}
            className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
          >
            <Download size={13} /> {t("exportDownload")}
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4">
          <p className="mb-3 text-sm text-[var(--color-fg-muted)]">
            {t("exportSongIntro")}
          </p>
          <pre className="whitespace-pre-wrap rounded-md bg-[var(--color-bg)] p-3 font-mono text-xs text-[var(--color-fg-muted)]">
            {exportQuery.isLoading
              ? t("exGenerating")
              : content.trim() || t("exportEmpty")}
          </pre>
        </div>
      </div>
    </div>
  );
}
