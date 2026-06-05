/**
 * Service / Queue editor — Phase 5.
 *
 * The "Kø" (queue) the operator goes live with. A gudstjeneste is an ordered
 * list of items (songs, scripture, gaps); going live compiles them into cues.
 * This page builds that queue: add songs (with arrangement + key), reorder,
 * remove — and shows *what each song contributes* (slides per section) before
 * going live, via `service_cue_summary` (the same compilation the live engine
 * runs). It also renames/dates/deletes the service and imports a plan from
 * SundayPlan, and can hand the selected service straight to the live console.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  BookOpen,
  CalendarDays,
  ChevronDown,
  ClipboardPaste,
  Eye,
  GripVertical,
  Import,
  Megaphone,
  Music,
  Pause,
  Pencil,
  Play,
  Plus,
  Search,
  StickyNote,
  Trash2,
  X,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import type {
  BibleBook,
  BibleTranslation,
  Cue,
  Library,
  OutputAppearance,
  SearchResult,
  Service,
  ServiceItem,
  ServiceItemCues,
  SongArrangement,
} from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT, useLocale, type TKey } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";
import { DEFAULT_OUTPUT_APPEARANCE } from "@/lib/outputBridge";
import { SlideView } from "@/components/SlideView";
import { Button, Select } from "@/components/ui";
import { PlanPreviewModal } from "./PlanPreviewModal";

interface Props {
  library: Library;
  /** Hand the selected service to the live console. */
  onGoLive?: (service: Service) => void;
  /** Deep-link from search: select this service on mount/prop change. */
  openServiceId?: string | null;
  onDeepLinkDone?: () => void;
}

export function ServicesPage({
  library,
  onGoLive,
  openServiceId,
  onDeepLinkDone,
}: Props) {
  const t = useT();
  const lang = useLocale((s) => s.lang);
  const qc = useQueryClient();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [planPreviewOpen, setPlanPreviewOpen] = useState(false);
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

  // Deep-link from search: select the service once the list is loaded.
  useEffect(() => {
    if (!openServiceId) return;
    if (services.some((s) => s.id === openServiceId)) {
      setSelectedId(openServiceId);
      onDeepLinkDone?.();
    } else if (servicesQuery.data) {
      onDeepLinkDone?.(); // unknown id — don't get stuck
    }
  }, [openServiceId, services, servicesQuery.data, onDeepLinkDone]);

  const createService = useMutation({
    mutationFn: () =>
      ipc.service.create(
        library.id,
        t("svcNewDefaultTitle", {
          date: new Date().toLocaleDateString(lang),
        }),
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
      const bits = [t("importMatchedSongs", { n: res.matched_songs })];
      if (res.created_songs.length)
        bits.push(t("importCreatedEmpty", { n: res.created_songs.length }));
      if (res.warnings.length)
        bits.push(t("importWarnings", { n: res.warnings.length }));
      setToast(
        t("importToast", {
          name: res.service.name,
          details: bits.join(", "),
        }),
      );
    },
    onError: (e) => setToast(t("importFailed", { error: String(e) })),
  });

  function onPickFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = ""; // allow re-importing the same file
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => importPlan.mutate(String(reader.result ?? ""));
    reader.onerror = () => setToast(t("importReadError"));
    reader.readAsText(file);
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">
          {t("navServices")}
        </h1>
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
            {importPlan.isPending
              ? t("svcImporting")
              : t("svcImportFromSundayPlan")}
          </span>
        </button>
        <button
          type="button"
          onClick={() => setPlanPreviewOpen(true)}
          className="flex items-center gap-1.5 rounded-md border border-[var(--color-border)] px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <ClipboardPaste size={14} aria-hidden />
          <span>{t("planPreviewButton")}</span>
        </button>
        <button
          type="button"
          onClick={() => createService.mutate()}
          disabled={createService.isPending}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          <Plus size={14} aria-hidden />
          <span>{t("svcNewService")}</span>
        </button>
      </header>

      <PlanPreviewModal
        library={library}
        open={planPreviewOpen}
        onClose={() => setPlanPreviewOpen(false)}
      />

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
              {t("svcListEmpty")}
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
                        {formatDate(Number(svc.starts_at), lang)}
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
            onChanged={() => servicesQuery.refetch()}
            onDeleted={async () => {
              setSelectedId(null);
              await servicesQuery.refetch();
              setToast(t("svcDeletedToast"));
            }}
          />
        ) : (
          <div className="grid place-items-center text-sm text-[var(--color-fg-muted)]">
            <p>{t("svcSelectOrCreate")}</p>
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
  onChanged,
  onDeleted,
}: {
  service: Service;
  library: Library;
  onGoLive?: (service: Service) => void;
  onChanged: () => void;
  onDeleted: () => void;
}) {
  const t = useT();
  const qc = useQueryClient();
  const [adding, setAdding] = useState<null | "song" | "scripture">(null);
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  const [notesOpen, setNotesOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [previewId, setPreviewId] = useState<string | null>(null);
  const [dragIndex, setDragIndex] = useState<number | null>(null);

  const summaryQuery = useQuery({
    queryKey: ["cueSummary", service.id],
    queryFn: () => ipc.service.cueSummary(service.id),
  });
  const itemsQuery = useQuery({
    queryKey: ["serviceItems", service.id],
    queryFn: () => ipc.service.items(service.id),
  });
  const summary = summaryQuery.data;
  const items = summary?.items ?? [];

  // Map service_item_id → full item, for showing the key and editing in place.
  const itemById = useMemo(() => {
    const m = new Map<string, ServiceItem>();
    for (const it of itemsQuery.data ?? []) m.set(it.id, it);
    return m;
  }, [itemsQuery.data]);

  function refresh() {
    void qc.invalidateQueries({ queryKey: ["cueSummary", service.id] });
    void qc.invalidateQueries({ queryKey: ["serviceItems", service.id] });
  }

  const addSong = useMutation({
    mutationFn: (a: {
      songId: string;
      arrangementId: string | null;
      key: string | null;
    }) => ipc.service.addSong(service.id, a.songId, a.arrangementId, a.key),
    onSuccess: () => refresh(),
  });
  const addNonSong = useMutation({
    mutationFn: (a: { kind: string; label: string }) =>
      ipc.service.addItem(service.id, a.kind, a.label),
    onSuccess: () => refresh(),
  });
  const addScripture = useMutation({
    mutationFn: (a: {
      translationId: string;
      book: string;
      chapter: number;
      verseStart: number | null;
      verseEnd: number | null;
    }) =>
      ipc.bible.addToService(
        service.id,
        a.translationId,
        a.book,
        a.chapter,
        a.verseStart,
        a.verseEnd,
      ),
    onSuccess: () => refresh(),
  });
  const updateItem = useMutation({
    mutationFn: (a: {
      itemId: string;
      arrangementId: string | null;
      key: string | null;
      notes: string | null;
    }) => ipc.service.updateItem(a.itemId, a.arrangementId, a.key, a.notes),
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
  const rename = useMutation({
    mutationFn: (name: string) => ipc.service.rename(service.id, name),
    onSuccess: () => onChanged(),
  });
  const setDate = useMutation({
    mutationFn: (ms: number) => ipc.service.setStartsAt(service.id, ms),
    onSuccess: () => onChanged(),
  });
  const setNotes = useMutation({
    mutationFn: (notes: string) => ipc.service.setNotes(service.id, notes),
    onSuccess: () => onChanged(),
  });
  const del = useMutation({
    mutationFn: () => ipc.service.delete(service.id),
    onSuccess: () => onDeleted(),
  });

  function move(index: number, dir: -1 | 1) {
    reorderTo(index, index + dir);
  }

  /** Move the item at `from` to position `to`, then persist the new order. */
  function reorderTo(from: number, to: number) {
    if (to < 0 || to >= items.length || from === to) return;
    const ids = items.map((i) => i.service_item_id);
    const [moved] = ids.splice(from, 1);
    ids.splice(to, 0, moved);
    reorder.mutate(ids);
  }

  return (
    <section className="flex min-h-0 flex-col">
      {/* Header */}
      <div className="flex items-start gap-3 border-b border-[var(--color-border)] px-6 py-3">
        <div className="min-w-0 flex-1">
          <EditableName
            value={service.name}
            onCommit={(name) => name !== service.name && rename.mutate(name)}
          />
          <div className="mt-1 flex items-center gap-3 text-xs text-[var(--color-fg-muted)]">
            <input
              type="date"
              value={toDateInput(Number(service.starts_at))}
              onChange={(e) => {
                const ms = fromDateInput(e.target.value);
                if (ms != null) setDate.mutate(ms);
              }}
              className="rounded border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-0.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
            />
            <span>
              {summary
                ? `${t(
                    items.length === 1
                      ? "svcElementCountOne"
                      : "svcElementCountMany",
                    { n: items.length },
                  )} · ${t("svcCuesInQueue", { n: summary.total_cues })}`
                : t("svcLoadingQueue")}
            </span>
          </div>
        </div>

        <button
          type="button"
          onClick={() => setNotesOpen((v) => !v)}
          title={t("svcNotes")}
          className={cn(
            "rounded-md p-2 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]",
            notesOpen && "bg-[var(--color-bg-surface)] text-[var(--color-fg)]",
          )}
        >
          <StickyNote size={16} />
        </button>
        <button
          type="button"
          onClick={() => setConfirmDelete(true)}
          title={t("svcDeleteService")}
          className="rounded-md p-2 text-[var(--color-fg-muted)] hover:bg-[var(--color-danger)]/15 hover:text-[var(--color-danger)]"
        >
          <Trash2 size={16} />
        </button>
        <div className="relative">
          <Button
            size="sm"
            variant="outline"
            onClick={() => setAddMenuOpen((v) => !v)}
          >
            <Plus size={14} aria-hidden />
            {t("actionAdd")}
            <ChevronDown size={13} aria-hidden />
          </Button>
          {addMenuOpen && (
            <>
              <div
                className="fixed inset-0 z-10"
                onClick={() => setAddMenuOpen(false)}
                aria-hidden
              />
              <div className="absolute right-0 z-20 mt-1 w-44 overflow-hidden rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] py-1 shadow-[var(--shadow-elevated)]">
                <AddMenuItem
                  icon={Music}
                  label={t("kindSong")}
                  onClick={() => {
                    setAddMenuOpen(false);
                    setAdding("song");
                  }}
                />
                <AddMenuItem
                  icon={BookOpen}
                  label={t("kindScripture")}
                  onClick={() => {
                    setAddMenuOpen(false);
                    setAdding("scripture");
                  }}
                />
                <AddMenuItem
                  icon={Pause}
                  label={t("kindGap")}
                  onClick={() => {
                    setAddMenuOpen(false);
                    addNonSong.mutate({ kind: "gap", label: t("kindGap") });
                  }}
                />
                <AddMenuItem
                  icon={Megaphone}
                  label={t("kindAnnouncement")}
                  onClick={() => {
                    setAddMenuOpen(false);
                    addNonSong.mutate({
                      kind: "announcement",
                      label: t("kindAnnouncement"),
                    });
                  }}
                />
              </div>
            </>
          )}
        </div>
        {onGoLive && (
          <button
            type="button"
            onClick={() => onGoLive(service)}
            disabled={summary?.total_cues === 0}
            title={
              summary?.total_cues === 0
                ? t("svcQueueEmptyTooltip")
                : t("svcGoLiveTooltip")
            }
            className="flex items-center gap-1.5 rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-40"
          >
            <Play size={14} aria-hidden fill="currentColor" />
            {t("goLive")}
          </button>
        )}
      </div>

      {notesOpen && (
        <NotesEditor
          initial={service.notes ?? ""}
          onSave={(n) => {
            setNotes.mutate(n);
            setNotesOpen(false);
          }}
          onClose={() => setNotesOpen(false)}
        />
      )}

      {adding === "song" && (
        <AddSongPanel
          library={library}
          onAdd={(songId, arrangementId, key) => {
            addSong.mutate({ songId, arrangementId, key });
            setAdding(null);
          }}
          onClose={() => setAdding(null)}
        />
      )}
      {adding === "scripture" && (
        <AddScripturePanel
          onAdd={(a) => {
            addScripture.mutate(a);
            setAdding(null);
          }}
          onClose={() => setAdding(null)}
        />
      )}

      {/* Queue */}
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {summaryQuery.isLoading ? (
          <p className="p-6 text-center text-sm text-[var(--color-fg-muted)]">
            {t("loadingShort")}
          </p>
        ) : items.length === 0 ? (
          <div className="mx-auto max-w-md py-16 text-center">
            <div className="mx-auto mb-4 grid h-12 w-12 place-items-center rounded-xl bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]">
              <CalendarDays size={20} />
            </div>
            <h3 className="text-[var(--text-ui-lg)] font-semibold">
              {t("svcEmptyQueueTitle")}
            </h3>
            <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
              {t("svcEmptyQueueBody")}
            </p>
            <Button className="mt-5" onClick={() => setAdding("song")}>
              <Plus size={14} aria-hidden />
              {t("svcAddSong")}
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
                serviceItem={itemById.get(item.service_item_id) ?? null}
                dragging={dragIndex === i}
                onDragStart={() => setDragIndex(i)}
                onDragEnter={() => {
                  if (dragIndex !== null && dragIndex !== i) {
                    reorderTo(dragIndex, i);
                    setDragIndex(i);
                  }
                }}
                onDragEnd={() => setDragIndex(null)}
                onUp={() => move(i, -1)}
                onDown={() => move(i, 1)}
                onPreview={
                  item.cue_count > 0
                    ? () => setPreviewId(item.service_item_id)
                    : undefined
                }
                onRemove={() => removeItem.mutate(item.service_item_id)}
                onSave={(arrangementId, key, notes) =>
                  updateItem.mutate({
                    itemId: item.service_item_id,
                    arrangementId,
                    key,
                    notes,
                  })
                }
              />
            ))}
          </ol>
        )}
      </div>

      {confirmDelete && (
        <ConfirmDialog
          title={t("svcConfirmDeleteTitle")}
          body={t("svcConfirmDeleteBody", { name: service.name })}
          confirmLabel={t("actionDelete")}
          onConfirm={() => {
            setConfirmDelete(false);
            del.mutate();
          }}
          onCancel={() => setConfirmDelete(false)}
        />
      )}

      {previewId && (
        <CuePreviewModal
          serviceId={service.id}
          itemId={previewId}
          title={
            items.find((i) => i.service_item_id === previewId)?.title ??
            t("previewLabel")
          }
          onClose={() => setPreviewId(null)}
        />
      )}
    </section>
  );
}

const KIND_KEY: Record<string, TKey> = {
  song: "kindSong",
  scripture: "kindScripture",
  custom_deck: "kindCustomDeck",
  gap: "kindGap",
  announcement: "kindAnnouncement",
  video: "kindVideo",
};

function AddMenuItem({
  icon: Icon,
  label,
  onClick,
}: {
  icon: typeof Music;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm hover:bg-[var(--color-bg-surface)]"
    >
      <Icon size={14} aria-hidden className="text-[var(--color-fg-muted)]" />
      {label}
    </button>
  );
}

function QueueItemRow({
  index,
  count,
  item,
  serviceItem,
  dragging,
  onDragStart,
  onDragEnter,
  onDragEnd,
  onUp,
  onDown,
  onPreview,
  onRemove,
  onSave,
}: {
  index: number;
  count: number;
  item: ServiceItemCues;
  serviceItem: ServiceItem | null;
  dragging: boolean;
  onDragStart: () => void;
  onDragEnter: () => void;
  onDragEnd: () => void;
  onUp: () => void;
  onDown: () => void;
  onPreview?: () => void;
  onRemove: () => void;
  onSave: (
    arrangementId: string | null,
    key: string | null,
    notes: string | null,
  ) => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const keyOverride = serviceItem?.key_override ?? null;

  return (
    <li
      draggable
      onDragStart={onDragStart}
      onDragEnter={onDragEnter}
      onDragOver={(e) => e.preventDefault()}
      onDragEnd={onDragEnd}
      className={cn(
        "rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] transition-opacity",
        dragging && "opacity-40 ring-1 ring-[var(--color-accent)]",
      )}
    >
      <div className="flex items-start gap-2 p-3">
        <span
          className="flex cursor-grab items-center gap-1 pt-0.5 text-[var(--color-fg-muted)] active:cursor-grabbing"
          title={t("dragReorder")}
        >
          <GripVertical size={14} aria-hidden />
          <span className="w-4 text-center font-mono text-xs tabular-nums">
            {index + 1}
          </span>
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="rounded bg-[var(--color-bg-surface)] px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-[var(--color-fg-muted)] uppercase">
              {KIND_KEY[item.kind] ? t(KIND_KEY[item.kind]) : item.kind}
            </span>
            <span className="truncate font-medium">{item.title}</span>
            {keyOverride && (
              <span className="rounded bg-[var(--color-accent)]/15 px-1.5 py-0.5 font-mono text-[11px] text-[var(--color-accent-fg)]">
                {keyOverride}
              </span>
            )}
            <span className="ml-auto shrink-0 text-xs text-[var(--color-fg-muted)]">
              {t(item.cue_count === 1 ? "slideCountOne" : "slideCountMany", {
                n: item.cue_count,
              })}
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
                  <span className="text-[var(--color-fg)]">
                    {localizeSectionLabel(sec.label, t)}
                  </span>
                  <span className="font-mono">×{sec.slide_count}</span>
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-0.5">
          {onPreview && (
            <button
              type="button"
              onClick={onPreview}
              title={t("actionPreview")}
              className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
            >
              <Eye size={14} />
            </button>
          )}
          {serviceItem && (
            <button
              type="button"
              onClick={() => setEditing((v) => !v)}
              title={t("actionEdit")}
              className={cn(
                "rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]",
                editing &&
                  "bg-[var(--color-bg-surface)] text-[var(--color-fg)]",
              )}
            >
              <Pencil size={14} />
            </button>
          )}
          <button
            type="button"
            onClick={onUp}
            disabled={index === 0}
            title={t("svcMoveUp")}
            className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-30"
          >
            <ArrowUp size={14} />
          </button>
          <button
            type="button"
            onClick={onDown}
            disabled={index === count - 1}
            title={t("svcMoveDown")}
            className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-30"
          >
            <ArrowDown size={14} />
          </button>
          <button
            type="button"
            onClick={onRemove}
            title={t("svcRemoveFromQueue")}
            className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-danger)]/15 hover:text-[var(--color-danger)]"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      {editing && serviceItem && (
        <ItemEditor
          serviceItem={serviceItem}
          onCancel={() => setEditing(false)}
          onSave={(arrId, key, notes) => {
            onSave(arrId, key, notes);
            setEditing(false);
          }}
        />
      )}
    </li>
  );
}

/** Inline editor for an existing queue item: arrangement + key for songs, or a
 * label for pauses/announcements. */
function ItemEditor({
  serviceItem,
  onSave,
  onCancel,
}: {
  serviceItem: ServiceItem;
  onSave: (
    arrangementId: string | null,
    key: string | null,
    notes: string | null,
  ) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const isSong = serviceItem.kind === "song";
  const [arrangementId, setArrangementId] = useState(
    serviceItem.arrangement_id ?? "",
  );
  const [key, setKey] = useState(serviceItem.key_override ?? "");
  const [notes, setNotes] = useState(serviceItem.notes ?? "");

  const arrangementsQuery = useQuery({
    queryKey: ["arrangements", serviceItem.song_id],
    queryFn: () => ipc.arrangement.list(serviceItem.song_id as string),
    enabled: isSong && !!serviceItem.song_id,
  });
  const arrangements: SongArrangement[] = arrangementsQuery.data ?? [];

  return (
    <div className="border-t border-[var(--color-border)] bg-[var(--color-bg-surface)]/40 px-4 py-3">
      <div className="flex flex-wrap items-end gap-3">
        {isSong ? (
          <>
            <label className="text-xs text-[var(--color-fg-muted)]">
              <span className="mb-1 block">{t("arrTitle")}</span>
              <Select
                className="w-48"
                value={arrangementId}
                onChange={(e) => setArrangementId(e.target.value)}
              >
                <option value="">{t("svcDefaultAllSections")}</option>
                {arrangements.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                    {a.is_default ? " ★" : ""}
                  </option>
                ))}
              </Select>
            </label>
            <label className="text-xs text-[var(--color-fg-muted)]">
              <span className="mb-1 block">{t("colKey")}</span>
              <input
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder={t("svcKeyPlaceholder")}
                className="w-24 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
              />
            </label>
          </>
        ) : (
          <label className="flex-1 text-xs text-[var(--color-fg-muted)]">
            <span className="mb-1 block">{t("svcLabelText")}</span>
            <input
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              placeholder={t("svcLabelPlaceholder")}
              className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
            />
          </label>
        )}
        <div className="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={onCancel}>
            {t("actionCancel")}
          </Button>
          <Button
            size="sm"
            onClick={() =>
              isSong
                ? onSave(
                    arrangementId || null,
                    key.trim() ? key.trim() : null,
                    serviceItem.notes ?? null,
                  )
                : onSave(null, null, notes.trim() ? notes.trim() : null)
            }
          >
            {t("actionSave")}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** Click-to-edit service name; commits on blur or Enter, reverts on Escape. */
function EditableName({
  value,
  onCommit,
}: {
  value: string;
  onCommit: (name: string) => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);

  if (!editing) {
    return (
      <button
        type="button"
        onClick={() => setEditing(true)}
        title={t("svcClickToRename")}
        className="max-w-full truncate rounded px-1 text-left font-semibold hover:bg-[var(--color-bg-surface)]"
      >
        {value}
      </button>
    );
  }
  return (
    <input
      autoFocus
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        setEditing(false);
        if (draft.trim()) onCommit(draft.trim());
        else setDraft(value);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") {
          setDraft(value);
          setEditing(false);
        }
      }}
      className="w-full rounded border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-1 font-semibold focus:border-[var(--color-accent)] focus:outline-none"
    />
  );
}

function NotesEditor({
  initial,
  onSave,
  onClose,
}: {
  initial: string;
  onSave: (notes: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState(initial);
  return (
    <div className="border-b border-[var(--color-border)] bg-[var(--color-bg-surface)]/40 px-6 py-3">
      <textarea
        autoFocus
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        rows={3}
        placeholder={t("svcNotesPlaceholder")}
        className="w-full resize-y rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-3 py-2 text-sm placeholder:text-[var(--color-fg-muted)] focus:border-[var(--color-accent)] focus:outline-none"
      />
      <div className="mt-2 flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={onClose}>
          {t("actionCancel")}
        </Button>
        <Button size="sm" onClick={() => onSave(draft)}>
          {t("svcSaveNotes")}
        </Button>
      </div>
    </div>
  );
}

function AddSongPanel({
  library,
  onAdd,
  onClose,
}: {
  library: Library;
  onAdd: (
    songId: string,
    arrangementId: string | null,
    key: string | null,
  ) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [q, setQ] = useState("");
  const [picked, setPicked] = useState<{ id: string; title: string } | null>(
    null,
  );
  const searching = q.trim().length > 1;
  const searchQuery = useQuery({
    queryKey: ["songs", library.id, "search", q],
    queryFn: () => ipc.song.search(library.id, q, 50),
    enabled: searching && !picked,
  });
  const listQuery = useQuery({
    queryKey: ["songs", library.id],
    queryFn: () => ipc.song.list(library.id, 10000),
    enabled: !searching && !picked,
  });

  const rows: Array<{ id: string; title: string; snippet?: string }> = searching
    ? (searchQuery.data ?? []).map((r: SearchResult) => ({
        id: r.song_id,
        title: r.title,
        snippet: r.snippet,
      }))
    : (listQuery.data ?? []).map((s) => ({ id: s.id, title: s.title }));

  if (picked) {
    return (
      <SongConfig
        song={picked}
        onBack={() => setPicked(null)}
        onAdd={(arrId, key) => onAdd(picked.id, arrId, key)}
      />
    );
  }

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
            placeholder={t("svcSearchSongToAdd")}
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
            {searching ? t("cmdNoHits", { q }) : t("svcNoSongsInLibrary")}
          </li>
        ) : (
          rows.map((row) => (
            <li key={row.id}>
              <button
                type="button"
                onClick={() => setPicked({ id: row.id, title: row.title })}
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

/** Step 2 of add-song: choose arrangement + key before it enters the queue. */
function SongConfig({
  song,
  onAdd,
  onBack,
}: {
  song: { id: string; title: string };
  onAdd: (arrangementId: string | null, key: string | null) => void;
  onBack: () => void;
}) {
  const t = useT();
  const [arrangementId, setArrangementId] = useState<string>("");
  const [key, setKey] = useState("");

  const arrangementsQuery = useQuery({
    queryKey: ["arrangements", song.id],
    queryFn: () => ipc.arrangement.list(song.id),
  });
  const arrangements: SongArrangement[] = arrangementsQuery.data ?? [];

  return (
    <div className="border-b border-[var(--color-border)] bg-[var(--color-bg-surface)]/40 px-6 py-3">
      <div className="mb-3 flex items-center gap-2">
        <button
          type="button"
          onClick={onBack}
          className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          title={t("svcBackToSearch")}
        >
          <ArrowUp size={14} className="-rotate-90" />
        </button>
        <span className="truncate font-medium">{song.title}</span>
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <label className="text-xs text-[var(--color-fg-muted)]">
          <span className="mb-1 block">{t("arrTitle")}</span>
          <Select
            className="w-48"
            value={arrangementId}
            onChange={(e) => setArrangementId(e.target.value)}
          >
            <option value="">{t("svcDefaultAllSections")}</option>
            {arrangements.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
                {a.is_default ? " ★" : ""}
              </option>
            ))}
          </Select>
        </label>
        <label className="text-xs text-[var(--color-fg-muted)]">
          <span className="mb-1 block">{t("svcKeyOptional")}</span>
          <input
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={t("svcKeyPlaceholder")}
            className="w-24 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
          />
        </label>
        <Button
          className="ml-auto"
          onClick={() =>
            onAdd(arrangementId || null, key.trim() ? key.trim() : null)
          }
        >
          <Plus size={14} aria-hidden />
          {t("svcAddToQueue")}
        </Button>
      </div>
    </div>
  );
}

function ConfirmDialog({
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onCancel}
        aria-hidden
      />
      <div className="relative w-[min(90vw,420px)] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-[var(--shadow-elevated)]">
        <h2 className="font-semibold">{title}</h2>
        <p className="mt-1 text-sm text-[var(--color-fg-muted)]">{body}</p>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={onCancel}>
            Avbryt
          </Button>
          <button
            type="button"
            onClick={onConfirm}
            className="rounded-md bg-[var(--color-danger)] px-4 py-1.5 text-sm font-semibold text-white hover:brightness-110"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/** Step 1 of add-scripture: pick translation → book → chapter → verses. Adds via
 *  the existing bible_add_to_service command. */
function AddScripturePanel({
  onAdd,
  onClose,
}: {
  onAdd: (a: {
    translationId: string;
    book: string;
    chapter: number;
    verseStart: number | null;
    verseEnd: number | null;
  }) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [translationId, setTranslationId] = useState("");
  const [book, setBook] = useState("");
  const [chapter, setChapter] = useState<number | "">("");
  const [verseStart, setVerseStart] = useState("");
  const [verseEnd, setVerseEnd] = useState("");

  const translations = useQuery({
    queryKey: ["bibleTranslations"],
    queryFn: () => ipc.bible.translations(),
  });
  // Default to the first installed translation.
  useEffect(() => {
    const list = translations.data;
    if (list && list.length && !translationId) setTranslationId(list[0].id);
  }, [translations.data, translationId]);

  const books = useQuery({
    queryKey: ["bibleBooks", translationId],
    queryFn: () => ipc.bible.books(translationId),
    enabled: !!translationId,
  });
  const chapters = useQuery({
    queryKey: ["bibleChapters", translationId, book],
    queryFn: () => ipc.bible.chapters(translationId, book),
    enabled: !!translationId && !!book,
  });

  const canAdd = !!translationId && !!book && chapter !== "";

  return (
    <div className="border-b border-[var(--color-border)] bg-[var(--color-bg-surface)]/40 px-6 py-3">
      <div className="flex flex-wrap items-end gap-3">
        <Field label={t("svcTranslation")}>
          <Select
            className="w-40"
            value={translationId}
            onChange={(e) => {
              setTranslationId(e.target.value);
              setBook("");
              setChapter("");
            }}
          >
            {(translations.data ?? []).map((t: BibleTranslation) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("svcBook")}>
          <Select
            className="w-40"
            value={book}
            onChange={(e) => {
              setBook(e.target.value);
              setChapter("");
            }}
          >
            <option value="">{t("svcSelectEllipsis")}</option>
            {(books.data ?? []).map((b: BibleBook) => (
              <option key={b.book} value={b.book}>
                {b.display}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("svcChapter")}>
          <Select
            className="w-24"
            value={chapter === "" ? "" : String(chapter)}
            onChange={(e) =>
              setChapter(e.target.value ? Number(e.target.value) : "")
            }
            disabled={!book}
          >
            <option value="">—</option>
            {(chapters.data ?? []).map((c: number) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={t("svcVerseFrom")}>
          <input
            value={verseStart}
            onChange={(e) => setVerseStart(e.target.value.replace(/\D/g, ""))}
            placeholder={t("svcAll")}
            className="w-20 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
          />
        </Field>
        <Field label={t("svcVerseTo")}>
          <input
            value={verseEnd}
            onChange={(e) => setVerseEnd(e.target.value.replace(/\D/g, ""))}
            placeholder="—"
            className="w-20 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
          />
        </Field>
        <div className="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={onClose}>
            {t("actionCancel")}
          </Button>
          <Button
            size="sm"
            disabled={!canAdd}
            onClick={() =>
              onAdd({
                translationId,
                book,
                chapter: chapter as number,
                verseStart: verseStart ? Number(verseStart) : null,
                verseEnd: verseEnd ? Number(verseEnd) : null,
              })
            }
          >
            <Plus size={14} aria-hidden />
            {t("svcAddToQueue")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="text-xs text-[var(--color-fg-muted)]">
      <span className="mb-1 block">{label}</span>
      {children}
    </label>
  );
}

/** Renders every slide an item produces, styled exactly as the output will,
 *  by compiling the cue list and filtering to this item's slides. */
function CuePreviewModal({
  serviceId,
  itemId,
  title,
  onClose,
}: {
  serviceId: string;
  itemId: string;
  title: string;
  onClose: () => void;
}) {
  const t = useT();
  const cueList = useQuery({
    queryKey: ["cueList", serviceId],
    queryFn: () => ipc.live.compileCueList(serviceId),
  });
  const appearance = useQuery({
    queryKey: ["outputAppearance"],
    queryFn: () => ipc.output.appearance(),
  });
  const app: OutputAppearance = appearance.data ?? DEFAULT_OUTPUT_APPEARANCE;

  const slides = (cueList.data?.cues ?? []).filter(
    (c: Cue): c is Extract<Cue, { kind: "show_slide" }> =>
      c.kind === "show_slide" && c.source.service_item_id === itemId,
  );

  return (
    <div className="fixed inset-0 z-50 grid place-items-center">
      <div
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative flex max-h-[85vh] w-[min(92vw,920px)] flex-col rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-5 py-3">
          <Eye size={16} className="text-[var(--color-accent)]" aria-hidden />
          <h2 className="flex-1 truncate font-semibold">{title}</h2>
          <span className="text-xs text-[var(--color-fg-muted)]">
            {t(slides.length === 1 ? "slideCountOne" : "slideCountMany", {
              n: slides.length,
            })}
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={16} />
          </button>
        </div>
        <div className="grid grid-cols-2 gap-3 overflow-y-auto p-5 sm:grid-cols-3">
          {cueList.isLoading ? (
            <p className="col-span-full py-8 text-center text-sm text-[var(--color-fg-muted)]">
              {t("svcCompiling")}
            </p>
          ) : slides.length === 0 ? (
            <p className="col-span-full py-8 text-center text-sm text-[var(--color-fg-muted)]">
              {t("svcNoSlidesToShow")}
            </p>
          ) : (
            slides.map((cue, i) => (
              <div key={cue.cue_id}>
                <div className="overflow-hidden rounded-md ring-1 ring-[var(--color-border)]">
                  <div className="aspect-video w-full">
                    <SlideView
                      frame={{
                        kind: "slide",
                        slide_content: cue.slide_content,
                      }}
                      appearance={app}
                      forceSectionLabel
                      localizeLabel={(l) => localizeSectionLabel(l, t)}
                    />
                  </div>
                </div>
                <div className="mt-1 text-center text-[10px] text-[var(--color-fg-muted)]">
                  {i + 1}
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function formatDate(ms: number, lang: string): string {
  return new Date(ms).toLocaleDateString(lang, {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}

/** millis → yyyy-mm-dd in local time, for <input type="date">. */
function toDateInput(ms: number): string {
  const d = new Date(ms);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** yyyy-mm-dd → millis at local noon (avoids timezone day-shift). */
function fromDateInput(v: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(v);
  if (!m) return null;
  return new Date(
    Number(m[1]),
    Number(m[2]) - 1,
    Number(m[3]),
    12,
    0,
  ).getTime();
}
