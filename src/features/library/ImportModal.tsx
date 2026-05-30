/**
 * ImportModal — Phase 2.2 song import.
 *
 * Reads song files with a plain `<input type="file">` + `FileReader` (so no
 * native file-dialog plugin is needed — it works in the Tauri webview today),
 * then sends each file's text to the `import_song_file` command, which detects
 * the format and creates the song. Supported: plain text, ChordPro, OpenSong
 * and OpenLyrics (OpenLP). Shows a per-file result with the detected format,
 * section count and any parser warnings.
 */

import { useRef, useState } from "react";
import { Download, FileText, X } from "lucide-react";

import type { ImportFormat } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { useT } from "@/lib/i18n";

const ACCEPT = ".txt,.cho,.crd,.chopro,.chordpro,.xml,.opensong,.pro_";

const FORMAT_LABEL: Record<ImportFormat, string> = {
  plain_text: "Tekst",
  chord_pro: "ChordPro",
  open_song: "OpenSong",
  open_lyrics: "OpenLyrics",
};

interface FileResult {
  name: string;
  ok: boolean;
  format?: ImportFormat;
  title?: string;
  sections?: number;
  warnings?: string[];
  error?: string;
}

interface ImportModalProps {
  libraryId: string;
  onClose: () => void;
  /** Called after at least one song was imported, so the list can refresh. */
  onImported: () => void;
}

function readAsText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsText(file);
  });
}

export function ImportModal({
  libraryId,
  onClose,
  onImported,
}: ImportModalProps) {
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<FileResult[]>([]);

  async function handleFiles(files: FileList | null) {
    if (!files || files.length === 0) return;
    setBusy(true);
    const collected: FileResult[] = [];
    let anyImported = false;

    for (const file of Array.from(files)) {
      try {
        const content = await readAsText(file);
        const res = await ipc.song.importFile(libraryId, file.name, content);
        anyImported = true;
        collected.push({
          name: file.name,
          ok: true,
          format: res.format,
          title: res.title,
          sections: res.section_count,
          warnings: res.warnings,
        });
      } catch (e) {
        collected.push({
          name: file.name,
          ok: false,
          error: e instanceof Error ? e.message : String(e),
        });
      }
      setResults([...collected]);
    }

    setBusy(false);
    if (anyImported) onImported();
  }

  const okCount = results.filter((r) => r.ok).length;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
      <button
        type="button"
        aria-label={t("actionClose")}
        onClick={onClose}
        className="absolute inset-0 bg-black/50"
      />
      <div className="relative flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <header className="flex items-center gap-2 border-b border-[var(--color-border)] px-4 py-3">
          <Download size={16} className="text-[var(--color-accent)]" />
          <h2 className="font-semibold">{t("importTitle")}</h2>
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            className="grid h-7 w-7 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={15} />
          </button>
        </header>

        <div className="flex flex-col gap-4 overflow-y-auto p-4">
          <p className="text-sm text-[var(--color-fg-muted)]">
            {t("importIntro")}
          </p>

          <input
            ref={inputRef}
            type="file"
            multiple
            accept={ACCEPT}
            className="hidden"
            onChange={(e) => void handleFiles(e.target.files)}
          />
          <button
            type="button"
            disabled={busy}
            onClick={() => inputRef.current?.click()}
            className="inline-flex w-fit items-center gap-2 rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
          >
            <FileText size={14} aria-hidden />
            {busy ? t("importBusy") : t("importChoose")}
          </button>

          {results.length > 0 && (
            <ul className="flex flex-col gap-1.5">
              {results.map((r, i) => (
                <li
                  key={`${r.name}-${i}`}
                  className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-3 py-2 text-sm"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={
                        r.ok
                          ? "text-[var(--color-accent)]"
                          : "text-[var(--color-warning)]"
                      }
                    >
                      {r.ok ? "✓" : "✕"}
                    </span>
                    <span className="font-medium">{r.title ?? r.name}</span>
                    {r.ok && r.format && (
                      <span className="rounded-full bg-[var(--color-bg-elevated)] px-2 py-0.5 text-[10px] text-[var(--color-fg-muted)]">
                        {FORMAT_LABEL[r.format]}
                      </span>
                    )}
                    {r.ok && (
                      <span className="text-xs text-[var(--color-fg-muted)]">
                        {r.sections && r.sections > 0
                          ? t("importSections", { n: r.sections })
                          : t("importNoContent")}
                      </span>
                    )}
                    {!r.ok && (
                      <span className="text-xs text-[var(--color-warning)]">
                        {t("importError")}
                      </span>
                    )}
                  </div>
                  {r.warnings && r.warnings.length > 0 && (
                    <ul className="mt-1 ml-6 list-disc text-xs text-[var(--color-fg-muted)]">
                      {r.warnings.map((w, wi) => (
                        <li key={wi}>{w}</li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        <footer className="flex items-center gap-3 border-t border-[var(--color-border)] px-4 py-3">
          {results.length > 0 && (
            <span className="text-xs text-[var(--color-fg-muted)]">
              {t("importDoneCount", { n: okCount, total: results.length })}
            </span>
          )}
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            className="rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110"
          >
            {t("importClose")}
          </button>
        </footer>
      </div>
    </div>
  );
}
