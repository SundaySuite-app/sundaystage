/**
 * Media browser — Phase 7.2.
 *
 * Grid of imported assets with a type filter, a broken-path badge, and a
 * hash-based relink action (the feature ProPresenter/OpenLP get wrong). Real
 * thumbnails (ffmpeg) and a native file-picker are follow-ups; for now assets
 * show a kind icon and import takes an absolute path.
 */

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Image as ImageIcon,
  Link2,
  Music,
  Plus,
  Trash2,
  Video,
} from "lucide-react";

import type { Library, MediaStatus } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { cn } from "@/lib/cn";

type Filter = "all" | "image" | "video" | "audio";

function basename(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function KindIcon({ kind }: { kind: string }) {
  if (kind === "video") return <Video size={22} />;
  if (kind === "audio") return <Music size={22} />;
  return <ImageIcon size={22} />;
}

interface Props {
  library: Library;
}

export function MediaPage({ library }: Props) {
  const qc = useQueryClient();
  const mediaKey = ["media", library.id] as const;
  const [filter, setFilter] = useState<Filter>("all");
  const [path, setPath] = useState("");

  const mediaQuery = useQuery({ queryKey: mediaKey, queryFn: () => ipc.media.list(library.id) });
  const items = useMemo(() => mediaQuery.data ?? [], [mediaQuery.data]);
  const shown = items.filter((m) => filter === "all" || m.asset.kind === filter);

  const importMut = useMutation({
    mutationFn: (p: string) => ipc.media.import(library.id, p),
    onSuccess: () => {
      setPath("");
      void qc.invalidateQueries({ queryKey: mediaKey });
    },
  });
  const deleteMut = useMutation({
    mutationFn: (id: string) => ipc.media.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: mediaKey }),
  });
  const relinkMut = useMutation({
    mutationFn: ({ id, dirs }: { id: string; dirs: string[] }) => ipc.media.relink(id, dirs),
    onSuccess: (asset) => {
      void qc.invalidateQueries({ queryKey: mediaKey });
      if (!asset) window.alert("Fant ingen fil med samme innhold i den mappen.");
    },
  });

  const relink = (id: string) => {
    const dir = window.prompt("Søk i hvilken mappe etter den flyttede filen?");
    if (dir?.trim()) relinkMut.mutate({ id, dirs: [dir.trim()] });
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">Media</h1>
        <span className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-xs text-[var(--color-fg-muted)]">
          {items.length} filer
        </span>
        <div className="flex-1" />
        <input
          type="text"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && path.trim() && importMut.mutate(path.trim())}
          placeholder="/sti/til/fil.png eller .mp4"
          className="w-72 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-3 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
        />
        <button
          type="button"
          onClick={() => path.trim() && importMut.mutate(path.trim())}
          disabled={!path.trim() || importMut.isPending}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          <Plus size={14} /> Importer
        </button>
      </header>

      <div className="flex items-center gap-1.5 border-b border-[var(--color-border)] px-6 py-2">
        {(["all", "image", "video", "audio"] as Filter[]).map((f) => (
          <button
            key={f}
            type="button"
            onClick={() => setFilter(f)}
            className={cn(
              "rounded-full px-3 py-1 text-xs transition-colors",
              filter === f
                ? "bg-[var(--color-accent)] text-[var(--color-sunday-blue-900)]"
                : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
            )}
          >
            {f === "all" ? "Alle" : f === "image" ? "Bilder" : f === "video" ? "Video" : "Lyd"}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {importMut.isError && (
          <p className="mb-3 text-sm text-[var(--color-danger)]">
            Import feilet: {String(importMut.error)}
          </p>
        )}
        {shown.length === 0 ? (
          <p className="text-sm text-[var(--color-fg-muted)]">
            {items.length === 0
              ? "Ingen media importert enda. Lim inn en filsti over for å starte."
              : "Ingen filer i dette filteret."}
          </p>
        ) : (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3">
            {shown.map((m) => (
              <MediaCard key={m.asset.id} status={m} onDelete={() => deleteMut.mutate(m.asset.id)} onRelink={() => relink(m.asset.id)} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function MediaCard({
  status,
  onDelete,
  onRelink,
}: {
  status: MediaStatus;
  onDelete: () => void;
  onRelink: () => void;
}) {
  const { asset, present } = status;
  return (
    <li className="group relative flex flex-col overflow-hidden rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)]">
      <div className="grid aspect-video place-items-center bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]">
        <KindIcon kind={asset.kind} />
      </div>
      <div className="flex items-center gap-1.5 p-2.5">
        <span className="flex-1 truncate text-xs" title={asset.original_path}>
          {basename(asset.original_path)}
        </span>
        {!present && (
          <span title="Filen finnes ikke på lagret sti" className="text-[var(--color-warning)]">
            <AlertTriangle size={13} />
          </span>
        )}
      </div>
      {!present && (
        <button
          type="button"
          onClick={onRelink}
          className="flex items-center justify-center gap-1.5 border-t border-[var(--color-border)] py-1.5 text-xs text-[var(--color-accent)] hover:bg-[var(--color-bg-surface)]"
        >
          <Link2 size={13} /> Koble på nytt
        </button>
      )}
      <button
        type="button"
        onClick={onDelete}
        title="Fjern"
        className="absolute right-1.5 top-1.5 grid h-6 w-6 place-items-center rounded-md bg-[var(--color-bg)]/70 text-[var(--color-fg-muted)] opacity-0 transition-opacity hover:text-[var(--color-danger)] group-hover:opacity-100"
      >
        <Trash2 size={13} />
      </button>
    </li>
  );
}
