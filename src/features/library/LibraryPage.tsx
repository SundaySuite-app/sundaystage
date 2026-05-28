/**
 * Song library page — Phase 2.2 of the build plan.
 *
 * For now: list view + create form, both calling the Rust IPC. Real
 * implementation needs virtualization (TanStack Virtual) and rich filters.
 */

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus, Search, Sparkles } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { Library, SearchResult, Song } from "@/lib/bindings";
import { SongEditor } from "./SongEditor";
import { PlanModal } from "./PlanModal";

interface Props {
  library: Library;
}

export function LibraryPage({ library }: Props) {
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [openSong, setOpenSong] = useState<{
    id: string;
    title: string;
  } | null>(null);
  const [planOpen, setPlanOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const songsQuery = useQuery({
    queryKey: ["songs", library.id],
    queryFn: () => ipc.song.list(library.id, 200),
  });

  const searchQuery = useQuery({
    queryKey: ["songs", library.id, "search", search],
    queryFn: () => ipc.song.search(library.id, search, 50),
    enabled: search.trim().length > 1,
  });

  const createSong = useMutation({
    mutationFn: () =>
      ipc.song.create({
        library_id: library.id,
        title: `Ny sang ${new Date().toLocaleTimeString("no")}`,
        language: "no",
        default_key: null,
        tempo_bpm: null,
        ccli_song_id: null,
        tono_work_id: null,
        copyright_notice: null,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["songs", library.id] }),
  });

  if (openSong) {
    return (
      <SongEditor
        songId={openSong.id}
        title={openSong.title}
        onBack={() => setOpenSong(null)}
      />
    );
  }

  const showingSearch = search.trim().length > 1;
  type Row = Partial<Song> & { id: string; title: string };
  const songs: Row[] = showingSearch
    ? (searchQuery.data ?? []).map<Row>((r: SearchResult) => ({
        id: r.song_id,
        title: r.title,
        // Full song data fetched on click — Phase 2.2 detail panel.
      }))
    : (songsQuery.data ?? []);

  return (
    <div className="flex h-full flex-col">
      {/* Top bar */}
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">
          Sangbibliotek
        </h1>
        <span className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-xs text-[var(--color-fg-muted)]">
          {library.name}
        </span>
        <div className="flex-1" />
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--color-fg-muted)]"
            aria-hidden
          />
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Søk i tekstlinjer…"
            className="w-72 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] py-1.5 pl-8 pr-3 text-sm placeholder:text-[var(--color-fg-muted)] focus:border-[var(--color-accent)] focus:outline-none"
          />
        </div>
        <button
          type="button"
          onClick={() => setPlanOpen(true)}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-border)] px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <Sparkles size={14} aria-hidden />
          <span>Planlegg med AI</span>
        </button>
        <button
          type="button"
          onClick={() => createSong.mutate()}
          disabled={createSong.isPending}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          <Plus size={14} aria-hidden />
          <span>Ny sang</span>
        </button>
      </header>

      {planOpen && (
        <PlanModal
          library={library}
          onClose={() => setPlanOpen(false)}
          onCreated={(name) => setToast(`Tjeneste opprettet: ${name}`)}
        />
      )}
      {toast && (
        <div className="fixed bottom-4 left-1/2 z-50 -translate-x-1/2 rounded-lg border border-[var(--color-accent)]/40 bg-[var(--color-bg-elevated)] px-4 py-2 text-sm shadow-[var(--shadow-elevated)]">
          {toast}
          <button
            onClick={() => setToast(null)}
            className="ml-3 text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]"
          >
            ✕
          </button>
        </div>
      )}

      {/* List */}
      <div className="flex-1 overflow-y-auto p-6">
        {songsQuery.isLoading && !showingSearch && (
          <p className="text-sm text-[var(--color-fg-muted)]">Laster sanger…</p>
        )}
        {songs.length === 0 && !songsQuery.isLoading && !showingSearch && (
          <EmptyState onCreate={() => createSong.mutate()} />
        )}
        {showingSearch && searchQuery.data?.length === 0 && (
          <p className="text-sm text-[var(--color-fg-muted)]">
            Ingen treff på «{search}».
          </p>
        )}
        {songs.length > 0 && (
          <ul className="space-y-1">
            {songs.map((song: Row) => (
              <li
                key={song.id}
                onClick={() => setOpenSong({ id: song.id, title: song.title })}
                className="flex cursor-pointer items-center gap-3 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-4 py-3 hover:border-[var(--color-accent)]/40 transition-colors"
              >
                <span className="font-medium">{song.title}</span>
                {song.default_key ? (
                  <span className="rounded bg-[var(--color-bg-surface)] px-1.5 py-0.5 text-[10px] font-mono text-[var(--color-fg-muted)]">
                    {song.default_key}
                  </span>
                ) : null}
                {song.tempo_bpm ? (
                  <span className="text-xs text-[var(--color-fg-muted)]">
                    {song.tempo_bpm} BPM
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="mx-auto max-w-md py-16 text-center">
      <div className="mx-auto mb-4 grid h-12 w-12 place-items-center rounded-xl bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]">
        <Search size={20} />
      </div>
      <h2 className="text-[var(--text-ui-lg)] font-semibold">
        Tomt bibliotek — la oss starte
      </h2>
      <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
        Legg til din første sang manuelt, eller importer fra ProPresenter,
        EasyWorship, FreeShow, OpenLP eller en mappe med tekstfiler.
      </p>
      <button
        type="button"
        onClick={onCreate}
        className="mt-5 inline-flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110"
      >
        <Plus size={14} aria-hidden />
        Lag din første sang
      </button>
    </div>
  );
}
