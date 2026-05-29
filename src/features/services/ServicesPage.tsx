/**
 * Service / Queue editor — Phase 5.
 *
 * The "Kø" (queue) the operator goes live with. A service is an ordered list of
 * items (songs, scripture, gaps); going live compiles them into cues. This page
 * is where you build that queue: add songs, reorder, remove — and crucially see
 * *what each song contributes* (how many slides per section) before you're live,
 * via `service_cue_summary` (the same compilation the live engine runs).
 *
 * It also imports a plan from SundayPlan (a JSON file), and can hand the
 * selected service straight to the live console.
 */
import { useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  CalendarDays,
  Import,
  Play,
  Plus,
  Search,
  Trash2,
  X,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import type {
  Library,
  SearchResult,
  Service,
  ServiceItemCues,
} from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { Button } from "@/components/ui";

interface Props {
  library: Library;
  /** Hand the selected service to the live console. */
  onGoLive?: (service: Service) => void;
}

export function ServicesPage({ library, onGoLive }: Props) {
  const qc = useQueryClient();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const servicesQuery = useQuery({
    queryKey: ["services", library.id],
    queryFn: () => ipc.service.upcoming(library.id, 0, 100),
  });
  const services = useMemo(
    () => servicesQuery.data ?? [],
    [servicesQuery.data],
  );
  const selected =
    services.find((s) => s.id === selectedId) ?? services[0] ?? null;

  const createService = useMutation({
    mutationFn: () =>
      ipc.service.create(
        library.id,
        `Ny tjeneste ${new Date().toLocaleDateString("no")}`,
        Date.now(),
      ),
    onSuccess: async (svc) => {
      await servicesQuery.refetch();
      setSelectedId(svc.id);
    },
  });

  const importPlan = useMutation({
    mutationFn: (json: string) =>
      ipc.service.importSundayPlan(library.id, json),
    onSuccess: async (res) => {
      await servicesQuery.refetch();
      void qc.invalidateQueries({ queryKey: ["songs", library.id] });
      setSelectedId(res.service.id);
      const bits = [`${res.matched_songs} sang(er) matchet`];
      if (res.created_songs.length)
        bits.push(`${res.created_songs.length} opprettet som tom`);
      if (res.warnings.length) bits.push(`${res.warnings.length} varsel`);
      setToast(`Importert «${res.service.name}» — ${bits.join(", ")}`);
    },
    onError: (e) => setToast(`Import feilet: ${String(e)}`),
  });

  function onPickFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = ""; // allow re-importing the same file
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => importPlan.mutate(String(reader.result ?? ""));
    reader.onerror = () => setToast("Kunne ikke lese filen");
    reader.readAsText(file);
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">Tjenester</h1>
        <span className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-xs text-[var(--color-fg-muted)]">
          {library.name}
        </span>
        <div className="flex-1" />
        <input
          ref={fileRef}
          type="file"
          accept="application/json,.json"
          className="hidden"
          onChange={onPickFile}
        />
        <button
          type="button"
          onClick={() => fileRef.current?.click()}
          disabled={importPlan.isPending}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-border)] px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-50"
        >
          <Import size={14} aria-hidden />
          <span>
            {importPlan.isPending ? "Importerer…" : "Importer fra SundayPlan"}
          </span>
        </button>
        <button
          type="button"
          onClick={() => createService.mutate()}
          disabled={createService.isPending}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          <Plus size={14} aria-hidden />
          <span>Ny tjeneste</span>
        </button>
      </header>

      {toast && (
        <div className="fixed bottom-4 left-1/2 z-50 flex max-w-[90vw] -translate-x-1/2 items-center gap-3 rounded-lg border border-[var(--color-accent)]/40 bg-[var(--color-bg-elevated)] px-4 py-2 text-sm shadow-[var(--shadow-elevated)]">
          <span>{toast}</span>
          <button
            onClick={() => setToast(null)}
            className="text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]"
          >
            <X size={14} />
          </button>
        </div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-[280px_1fr]">
        {/* Service list */}
        <aside className="min-h-0 overflow-y-auto border-r border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-2">
          {services.length === 0 ? (
            <p className="p-4 text-center text-sm text-[var(--color-fg-muted)]">
              Ingen tjenester enda. Lag en ny eller importer fra SundayPlan.
            </p>
          ) : (
            <ul className="space-y-0.5">
              {services.map((svc) => (
                <li key={svc.id}>
                  <button
                    type="button"
                    onClick={() => setSelectedId(svc.id)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors",
                      svc.id === selected?.id
                        ? "bg-[var(--color-accent)]/15 text-[var(--color-fg)] ring-1 ring-[var(--color-accent)]"
                        : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]",
                    )}
                  >
                    <CalendarDays size={14} aria-hidden className="shrink-0" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">
                        {svc.name}
                      </span>
                      <span className="block text-[11px] text-[var(--color-fg-muted)]">
                        {new Date(Number(svc.starts_at)).toLocaleDateString(
                          "no",
                          { weekday: "short", day: "numeric", month: "short" },
                        )}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        {/* Selected service editor */}
        {selected ? (
          <QueueEditor
            key={selected.id}
            service={selected}
            library={library}
            onGoLive={onGoLive}
          />
        ) : (
          <div className="grid place-items-center text-sm text-[var(--color-fg-muted)]">
            <p>Velg eller lag en tjeneste for å bygge køen.</p>
          </div>
        )}
      </div>
    </div>
  );
}

function QueueEditor({
  service,
  library,
  onGoLive,
}: {
  service: Service;
  library: Library;
  onGoLive?: (service: Service) => void;
}) {
  const qc = useQueryClient();
  const [adding, setAdding] = useState(false);

  const summaryQuery = useQuery({
    queryKey: ["cueSummary", service.id],
    queryFn: () => ipc.service.cueSummary(service.id),
  });
  const summary = summaryQuery.data;
  const items = summary?.items ?? [];

  function refresh() {
    return qc.invalidateQueries({ queryKey: ["cueSummary", service.id] });
  }

  const addSong = useMutation({
    mutationFn: (songId: string) => ipc.service.addSong(service.id, songId),
    onSuccess: () => refresh(),
  });
  const removeItem = useMutation({
    mutationFn: (itemId: string) => ipc.service.removeItem(itemId),
    onSuccess: () => refresh(),
  });
  const reorder = useMutation({
    mutationFn: (orderedIds: string[]) =>
      ipc.service.reorderItems(service.id, orderedIds),
    onSuccess: () => refresh(),
  });

  function move(index: number, dir: -1 | 1) {
    const target = index + dir;
    if (target < 0 || target >= items.length) return;
    const ids = items.map((i) => i.service_item_id);
    [ids[index], ids[target]] = [ids[target], ids[index]];
    reorder.mutate(ids);
  }

  return (
    <section className="flex min-h-0 flex-col">
      {/* Header */}
      <div className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-3">
        <div className="min-w-0">
          <h2 className="truncate font-semibold">{service.name}</h2>
          <p className="text-xs text-[var(--color-fg-muted)]">
            {summary
              ? `${items.length} element${items.length === 1 ? "" : "er"} · ${summary.total_cues} cues i køen`
              : "Laster kø…"}
          </p>
        </div>
        <div className="flex-1" />
        <Button
          size="sm"
          variant="outline"
          onClick={() => setAdding((v) => !v)}
        >
          <Plus size={14} aria-hidden />
          Legg til sang
        </Button>
        {onGoLive && (
          <button
            type="button"
            onClick={() => onGoLive(service)}
            disabled={summary?.total_cues === 0}
            title={
              summary?.total_cues === 0
                ? "Køen er tom — legg til innhold først"
                : "Gå live med denne tjenesten"
            }
            className="flex items-center gap-1.5 rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-40"
          >
            <Play size={14} aria-hidden fill="currentColor" />
            Gå live
          </button>
        )}
      </div>

      {adding && (
        <AddSongPanel
          library={library}
          onAdd={(songId) => addSong.mutate(songId)}
          onClose={() => setAdding(false)}
        />
      )}

      {/* Queue */}
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {summaryQuery.isLoading ? (
          <p className="p-6 text-center text-sm text-[var(--color-fg-muted)]">
            Laster…
          </p>
        ) : items.length === 0 ? (
          <div className="mx-auto max-w-md py-16 text-center">
            <div className="mx-auto mb-4 grid h-12 w-12 place-items-center rounded-xl bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]">
              <CalendarDays size={20} />
            </div>
            <h3 className="text-[var(--text-ui-lg)] font-semibold">Tom kø</h3>
            <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
              Legg til sanger så ser du her nøyaktig hvilke lysbilder hver sang
              blir, og hvor mange cues køen får totalt.
            </p>
            <Button className="mt-5" onClick={() => setAdding(true)}>
              <Plus size={14} aria-hidden />
              Legg til sang
            </Button>
          </div>
        ) : (
          <ol className="space-y-2">
            {items.map((item, i) => (
              <QueueItemRow
                key={item.service_item_id}
                index={i}
                count={items.length}
                item={item}
                onUp={() => move(i, -1)}
                onDown={() => move(i, 1)}
                onRemove={() => removeItem.mutate(item.service_item_id)}
              />
            ))}
          </ol>
        )}
      </div>
    </section>
  );
}

const KIND_LABEL: Record<string, string> = {
  song: "Sang",
  scripture: "Skrift",
  custom_deck: "Lysbilder",
  gap: "Pause",
  announcement: "Kunngjøring",
  video: "Video",
};

function QueueItemRow({
  index,
  count,
  item,
  onUp,
  onDown,
  onRemove,
}: {
  index: number;
  count: number;
  item: ServiceItemCues;
  onUp: () => void;
  onDown: () => void;
  onRemove: () => void;
}) {
  return (
    <li className="flex items-start gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
      <div className="flex w-6 shrink-0 flex-col items-center pt-0.5">
        <span className="font-mono text-xs tabular-nums text-[var(--color-fg-muted)]">
          {index + 1}
        </span>
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="rounded bg-[var(--color-bg-surface)] px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-[var(--color-fg-muted)] uppercase">
            {KIND_LABEL[item.kind] ?? item.kind}
          </span>
          <span className="truncate font-medium">{item.title}</span>
          <span className="ml-auto shrink-0 text-xs text-[var(--color-fg-muted)]">
            {item.cue_count} {item.cue_count === 1 ? "lysbilde" : "lysbilder"}
          </span>
        </div>
        {/* What goes into the queue for this item — per-section slide counts. */}
        {item.sections.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1.5">
            {item.sections.map((sec, si) => (
              <span
                key={si}
                className="inline-flex items-center gap-1 rounded-md bg-[var(--color-bg-surface)] px-2 py-0.5 text-[11px] text-[var(--color-fg-muted)]"
              >
                <span className="text-[var(--color-fg)]">{sec.label}</span>
                <span className="font-mono">×{sec.slide_count}</span>
              </span>
            ))}
          </div>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-0.5">
        <button
          type="button"
          onClick={onUp}
          disabled={index === 0}
          title="Flytt opp"
          className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-30"
        >
          <ArrowUp size={14} />
        </button>
        <button
          type="button"
          onClick={onDown}
          disabled={index === count - 1}
          title="Flytt ned"
          className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-30"
        >
          <ArrowDown size={14} />
        </button>
        <button
          type="button"
          onClick={onRemove}
          title="Fjern fra kø"
          className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-danger)]/15 hover:text-[var(--color-danger)]"
        >
          <Trash2 size={14} />
        </button>
      </div>
    </li>
  );
}

function AddSongPanel({
  library,
  onAdd,
  onClose,
}: {
  library: Library;
  onAdd: (songId: string) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  const searching = q.trim().length > 1;
  const searchQuery = useQuery({
    queryKey: ["songs", library.id, "search", q],
    queryFn: () => ipc.song.search(library.id, q, 50),
    enabled: searching,
  });
  const listQuery = useQuery({
    queryKey: ["songs", library.id],
    queryFn: () => ipc.song.list(library.id, 10000),
    enabled: !searching,
  });

  const rows: Array<{ id: string; title: string; snippet?: string }> = searching
    ? (searchQuery.data ?? []).map((r: SearchResult) => ({
        id: r.song_id,
        title: r.title,
        snippet: r.snippet,
      }))
    : (listQuery.data ?? []).map((s) => ({ id: s.id, title: s.title }));

  return (
    <div className="border-b border-[var(--color-border)] bg-[var(--color-bg-surface)]/40 px-6 py-3">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search
            size={14}
            className="absolute top-1/2 left-2.5 -translate-y-1/2 text-[var(--color-fg-muted)]"
            aria-hidden
          />
          <input
            autoFocus
            type="search"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Søk etter sang å legge til…"
            className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] py-1.5 pr-3 pl-8 text-sm placeholder:text-[var(--color-fg-muted)] focus:border-[var(--color-accent)] focus:outline-none"
          />
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <X size={16} />
        </button>
      </div>
      <ul className="mt-2 max-h-56 overflow-y-auto">
        {rows.length === 0 ? (
          <li className="px-2 py-4 text-center text-sm text-[var(--color-fg-muted)]">
            {searching
              ? `Ingen treff på «${q}».`
              : "Ingen sanger i biblioteket."}
          </li>
        ) : (
          rows.map((row) => (
            <li key={row.id}>
              <button
                type="button"
                onClick={() => onAdd(row.id)}
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-[var(--color-bg-surface)]"
              >
                <Plus
                  size={14}
                  aria-hidden
                  className="shrink-0 text-[var(--color-accent)]"
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{row.title}</span>
                  {row.snippet && (
                    <span className="block truncate text-[11px] text-[var(--color-fg-muted)]">
                      {row.snippet}
                    </span>
                  )}
                </span>
              </button>
            </li>
          ))
        )}
      </ul>
    </div>
  );
}
