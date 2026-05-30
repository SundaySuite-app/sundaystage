/**
 * Song library — Phase 2.2.
 *
 * A virtualized table (TanStack Virtual) that stays smooth at 10k+ songs, with
 * language/licensing filters, full-text search, and an inline preview pane that
 * shows the selected song's sections. Licensing is derived from CCLI/TONO ids
 * (live SundaySong coverage is a later integration).
 */
import { useMemo, useRef, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Plus, Search, Sparkles } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { Library, SongSection } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { Badge, Button, Select } from "@/components/ui";
import { useT } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";
import { SongEditor } from "./SongEditor";
import { PlanModal } from "./PlanModal";

interface Props {
  library: Library;
}

interface Row {
  id: string;
  title: string;
  key: string | null;
  tempo: number | null;
  language: string | null;
  licensing: "CCLI" | "TONO" | "Ukjent";
  lastUsed: number | null;
  snippet?: string;
}

const ROW_HEIGHT = 44;

export function LibraryPage({ library }: Props) {
  const t = useT();
  const qc = useQueryClient();
  const [search, setSearch] = useState("");
  const [lang, setLang] = useState("all");
  const [lic, setLic] = useState("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [openSong, setOpenSong] = useState<{
    id: string;
    title: string;
  } | null>(null);
  const [planOpen, setPlanOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const songsQuery = useQuery({
    queryKey: ["songs", library.id],
    queryFn: () => ipc.song.list(library.id, 10000),
  });
  const showingSearch = search.trim().length > 1;
  const searchQuery = useQuery({
    queryKey: ["songs", library.id, "search", search],
    queryFn: () => ipc.song.search(library.id, search, 200),
    enabled: showingSearch,
  });

  const allRows: Row[] = useMemo(() => {
    if (showingSearch) {
      const byId = new Map(
        (songsQuery.data ?? []).map((s) => [s.id, s] as const),
      );
      return (searchQuery.data ?? []).map((r) => {
        const s = byId.get(r.song_id);
        return {
          id: r.song_id,
          title: r.title,
          key: s?.default_key ?? null,
          tempo: s?.tempo_bpm != null ? Number(s.tempo_bpm) : null,
          language: s?.language ?? null,
          licensing: licensingOf(s),
          lastUsed: s?.last_used_at != null ? Number(s.last_used_at) : null,
          snippet: r.snippet,
        };
      });
    }
    return (songsQuery.data ?? []).map((s) => ({
      id: s.id,
      title: s.title,
      key: s.default_key,
      tempo: s.tempo_bpm != null ? Number(s.tempo_bpm) : null,
      language: s.language,
      licensing: licensingOf(s),
      lastUsed: s.last_used_at != null ? Number(s.last_used_at) : null,
    }));
  }, [showingSearch, songsQuery.data, searchQuery.data]);

  const languages = useMemo(
    () =>
      Array.from(
        new Set((songsQuery.data ?? []).map((s) => s.language)),
      ).sort(),
    [songsQuery.data],
  );

  const rows = useMemo(
    () =>
      allRows.filter(
        (r) =>
          (lang === "all" || r.language === lang) &&
          (lic === "all" || r.licensing === lic),
      ),
    [allRows, lang, lic],
  );

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  const createSong = useMutation({
    mutationFn: () =>
      ipc.song.create({
        library_id: library.id,
        title: t("newSongDefaultTitle", {
          time: new Date().toLocaleTimeString(),
        }),
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

  const empty =
    !songsQuery.isLoading &&
    !showingSearch &&
    (songsQuery.data ?? []).length === 0;

  return (
    <div className="flex h-full flex-col">
      {/* Top bar */}
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">
          {t("cmdSongLibrary")}
        </h1>
        <span className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-xs text-[var(--color-fg-muted)]">
          {library.name}
        </span>
        <div className="flex-1" />
        <div className="relative">
          <Search
            size={14}
            className="absolute top-1/2 left-2.5 -translate-y-1/2 text-[var(--color-fg-muted)]"
            aria-hidden
          />
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("librarySearchPlaceholder")}
            className="w-72 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] py-1.5 pr-3 pl-8 text-sm placeholder:text-[var(--color-fg-muted)] focus:border-[var(--color-accent)] focus:outline-none"
          />
        </div>
        <button
          type="button"
          onClick={() => setPlanOpen(true)}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-border)] px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <Sparkles size={14} aria-hidden />
          <span>{t("libraryPlanWithAi")}</span>
        </button>
        <button
          type="button"
          onClick={() => createSong.mutate()}
          disabled={createSong.isPending}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          <Plus size={14} aria-hidden />
          <span>{t("libraryNewSong")}</span>
        </button>
      </header>

      {/* Filters */}
      {!empty && (
        <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-6 py-2 text-xs">
          <Select
            className="w-36"
            value={lang}
            onChange={(e) => setLang(e.target.value)}
          >
            <option value="all">{t("filterAllLanguages")}</option>
            {languages.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </Select>
          <Select
            className="w-40"
            value={lic}
            onChange={(e) => setLic(e.target.value)}
          >
            <option value="all">{t("filterAllLicensing")}</option>
            <option value="CCLI">CCLI</option>
            <option value="TONO">TONO</option>
            <option value="Ukjent">{t("licenseUnknown")}</option>
          </Select>
          <span className="ml-auto text-[var(--color-fg-muted)]">
            {t(rows.length === 1 ? "songCountOne" : "songCountMany", {
              n: rows.length,
            })}
          </span>
        </div>
      )}

      {planOpen && (
        <PlanModal
          library={library}
          onClose={() => setPlanOpen(false)}
          onCreated={(name) => setToast(t("toastServiceCreated", { name }))}
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

      {empty ? (
        <div className="flex-1 overflow-y-auto p-6">
          <EmptyState onCreate={() => createSong.mutate()} />
        </div>
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[1fr_360px]">
          {/* Virtualized table */}
          <div className="flex min-h-0 flex-col">
            <div className="grid grid-cols-[1fr_4rem_4rem_3rem_5rem] gap-2 border-b border-[var(--color-border)] px-6 py-2 text-[10px] font-semibold uppercase tracking-wider text-[var(--color-fg-muted)]">
              <span>{t("colTitle")}</span>
              <span>{t("colKey")}</span>
              <span>{t("colTempo")}</span>
              <span>{t("colLanguage")}</span>
              <span>{t("colLicense")}</span>
            </div>
            <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
              {showingSearch && searchQuery.isLoading ? (
                <p className="p-6 text-sm text-[var(--color-fg-muted)]">
                  {t("cmdSearching")}
                </p>
              ) : rows.length === 0 ? (
                <p className="p-6 text-sm text-[var(--color-fg-muted)]">
                  {showingSearch
                    ? t("cmdNoHits", { q: search })
                    : t("libraryNoSongs")}
                </p>
              ) : (
                <div
                  style={{
                    height: virtualizer.getTotalSize(),
                    position: "relative",
                  }}
                >
                  {virtualizer.getVirtualItems().map((vi) => {
                    const row = rows[vi.index];
                    return (
                      <div
                        key={row.id}
                        onClick={() => setSelectedId(row.id)}
                        onDoubleClick={() =>
                          setOpenSong({ id: row.id, title: row.title })
                        }
                        className={cn(
                          "absolute top-0 left-0 grid w-full cursor-pointer grid-cols-[1fr_4rem_4rem_3rem_5rem] items-center gap-2 border-b border-[var(--color-border)] px-6 text-sm",
                          row.id === selectedId &&
                            "bg-[var(--color-accent)]/10 ring-1 ring-inset ring-[var(--color-accent)]/40",
                        )}
                        style={{
                          height: ROW_HEIGHT,
                          transform: `translateY(${vi.start}px)`,
                        }}
                      >
                        <span className="min-w-0">
                          <span className="block truncate font-medium">
                            {row.title}
                          </span>
                          {row.snippet && (
                            <span className="block truncate text-[11px] text-[var(--color-fg-muted)]">
                              {row.snippet}
                            </span>
                          )}
                        </span>
                        <span className="font-mono text-xs text-[var(--color-fg-muted)]">
                          {row.key ?? "—"}
                        </span>
                        <span className="text-xs text-[var(--color-fg-muted)]">
                          {row.tempo ?? "—"}
                        </span>
                        <span className="text-xs text-[var(--color-fg-muted)]">
                          {row.language ?? "—"}
                        </span>
                        <LicenseBadge value={row.licensing} />
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>

          {/* Preview pane */}
          <PreviewPane
            row={rows.find((r) => r.id === selectedId) ?? null}
            onEdit={(r) => setOpenSong({ id: r.id, title: r.title })}
          />
        </div>
      )}
    </div>
  );
}

function licensingOf(s?: {
  ccli_song_id: string | null;
  tono_work_id: string | null;
}): Row["licensing"] {
  if (!s) return "Ukjent";
  if (s.ccli_song_id) return "CCLI";
  if (s.tono_work_id) return "TONO";
  return "Ukjent";
}

function LicenseBadge({ value }: { value: Row["licensing"] }) {
  if (value === "Ukjent")
    return <span className="text-xs text-[var(--color-fg-muted)]">—</span>;
  return (
    <Badge variant={value === "CCLI" ? "accent" : "neutral"}>{value}</Badge>
  );
}

function PreviewPane({
  row,
  onEdit,
}: {
  row: Row | null;
  onEdit: (r: Row) => void;
}) {
  const t = useT();
  const sections = useQuery({
    queryKey: ["songSections", row?.id],
    queryFn: () => ipc.song.sections(row!.id),
    enabled: !!row,
  });

  if (!row) {
    return (
      <aside className="grid place-items-center border-l border-[var(--color-border)] p-6 text-center text-sm text-[var(--color-fg-muted)]">
        <p>{t("librarySelectForPreview")}</p>
      </aside>
    );
  }

  return (
    <aside className="flex min-h-0 flex-col border-l border-[var(--color-border)] bg-[var(--color-bg-elevated)]">
      <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-4 py-3">
        <h2 className="flex-1 truncate font-semibold">{row.title}</h2>
        <Button size="sm" onClick={() => onEdit(row)}>
          {t("actionEdit")}
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto p-4">
        {sections.isLoading ? (
          <p className="text-sm text-[var(--color-fg-muted)]">
            {t("loadingShort")}
          </p>
        ) : (sections.data ?? []).length === 0 ? (
          <p className="text-sm text-[var(--color-fg-muted)]">
            {t("libraryNoLyricsYet")}
          </p>
        ) : (
          <div className="space-y-3">
            {(sections.data ?? []).map((sec: SongSection) => (
              <div key={sec.id}>
                <div className="mb-1 text-[10px] font-semibold tracking-widest text-[var(--color-accent)] uppercase">
                  {localizeSectionLabel(sec.label, t)}
                </div>
                <pre className="font-sans text-xs whitespace-pre-wrap text-[var(--color-fg-muted)]">
                  {sec.lyrics}
                </pre>
              </div>
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  const t = useT();
  return (
    <div className="mx-auto max-w-md py-16 text-center">
      <div className="mx-auto mb-4 grid h-12 w-12 place-items-center rounded-xl bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]">
        <Search size={20} />
      </div>
      <h2 className="text-[var(--text-ui-lg)] font-semibold">
        {t("libraryEmptyTitle")}
      </h2>
      <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
        {t("libraryEmptyBody")}
      </p>
      <button
        type="button"
        onClick={onCreate}
        className="mt-5 inline-flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110"
      >
        <Plus size={14} aria-hidden />
        {t("libraryCreateFirst")}
      </button>
    </div>
  );
}
