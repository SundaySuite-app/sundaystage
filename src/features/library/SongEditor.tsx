/**
 * SongEditor — Phase 3.3 song structure editor.
 *
 * Two halves:
 *   - Sections: the reusable lyric blocks (verse 1, chorus, bridge …). Edit
 *     label + lyrics, add, delete, reorder.
 *   - Arrangement: an ordered, repeatable sequence of section references
 *     ("verse → chorus → verse → chorus"). Multiple arrangements per song;
 *     one is the default. Slides are generated from the resolved sequence, so
 *     editing a section's lyrics updates every place it appears.
 *
 * The generated-slide preview walks the active arrangement using the same
 * 4-lines-per-slide rule as the Rust cue compiler (`DEFAULT_LINES_PER_SLIDE`).
 */

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Copy, Plus, Sparkles, Star, Trash2, X } from "lucide-react";

import type { ArrangementItem, SongArrangement, SongSection } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { docWithText } from "@/lib/slideEditor/doc";
import { cn } from "@/lib/cn";
import { SlideCanvas } from "@/features/decks/SlideCanvas";
import { PasteFormatModal } from "./PasteFormatModal";

const LINES_PER_SLIDE = 4;
const SECTION_LABELS = [
  "intro",
  "verse_1",
  "verse_2",
  "verse_3",
  "pre_chorus",
  "chorus",
  "bridge",
  "instrumental",
  "tag",
  "ending",
];

function humanize(label: string): string {
  return label
    .split("_")
    .map((p) => (p ? p[0].toUpperCase() + p.slice(1) : ""))
    .join(" ");
}

function sectionToSlides(lyrics: string): string[][] {
  const lines = lyrics.split("\n").filter((l) => l.trim().length > 0);
  if (lines.length === 0) return [];
  const out: string[][] = [];
  for (let i = 0; i < lines.length; i += LINES_PER_SLIDE) {
    out.push(lines.slice(i, i + LINES_PER_SLIDE));
  }
  return out;
}

function slideCount(lyrics: string): number {
  return sectionToSlides(lyrics).length;
}

interface SongEditorProps {
  songId: string;
  title: string;
  onBack: () => void;
}

export function SongEditor({ songId, title, onBack }: SongEditorProps) {
  const qc = useQueryClient();
  const sectionsKey = ["sections", songId] as const;
  const arrangementsKey = ["arrangements", songId] as const;

  const sectionsQuery = useQuery({ queryKey: sectionsKey, queryFn: () => ipc.song.sections(songId) });
  const arrangementsQuery = useQuery({
    queryKey: arrangementsKey,
    queryFn: () => ipc.arrangement.list(songId),
  });
  const sections = useMemo(() => sectionsQuery.data ?? [], [sectionsQuery.data]);
  const arrangements = useMemo(() => arrangementsQuery.data ?? [], [arrangementsQuery.data]);

  const [activeArrId, setActiveArrId] = useState<string | null>(null);
  const [showPaste, setShowPaste] = useState(false);
  useEffect(() => {
    if (arrangements.length === 0) {
      setActiveArrId(null);
    } else if (!activeArrId || !arrangements.some((a) => a.id === activeArrId)) {
      const def = arrangements.find((a) => Number(a.is_default) === 1);
      setActiveArrId(def?.id ?? arrangements[0].id);
    }
  }, [arrangements, activeArrId]);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-4 py-3">
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <ArrowLeft size={15} /> Bibliotek
        </button>
        <h1 className="text-[var(--text-ui-lg)] font-semibold">{title}</h1>
        <div className="flex-1" />
        <button
          type="button"
          onClick={() => setShowPaste(true)}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110"
        >
          <Sparkles size={14} /> Lim inn &amp; formater med AI
        </button>
      </header>

      {showPaste && (
        <PasteFormatModal
          songId={songId}
          onClose={() => setShowPaste(false)}
          onApplied={(arrId) => {
            void qc.invalidateQueries({ queryKey: sectionsKey });
            void qc.invalidateQueries({ queryKey: arrangementsKey });
            setActiveArrId(arrId);
          }}
        />
      )}

      <div className="grid flex-1 grid-cols-[1fr_1fr] overflow-hidden">
        <SectionsPanel songId={songId} sections={sections} qc={qc} sectionsKey={sectionsKey} arrangementsKey={arrangementsKey} />
        <ArrangementPanel
          songId={songId}
          sections={sections}
          arrangements={arrangements}
          activeArrId={activeArrId}
          onActivate={setActiveArrId}
          qc={qc}
          arrangementsKey={arrangementsKey}
        />
      </div>
    </div>
  );
}

// ── Sections ───────────────────────────────────────────────────────────────────

type QC = ReturnType<typeof useQueryClient>;

function SectionsPanel({
  songId,
  sections,
  qc,
  sectionsKey,
  arrangementsKey,
}: {
  songId: string;
  sections: SongSection[];
  qc: QC;
  sectionsKey: readonly unknown[];
  arrangementsKey: readonly unknown[];
}) {
  const [dragFrom, setDragFrom] = useState<number | null>(null);

  const addMut = useMutation({
    mutationFn: () => ipc.song.addSection(songId, "verse_1", ""),
    onSuccess: () => qc.invalidateQueries({ queryKey: sectionsKey }),
  });
  const saveMut = useMutation({
    mutationFn: ({ id, label, lyrics }: { id: string; label: string; lyrics: string }) =>
      ipc.song.updateSection(id, label, lyrics),
    onSuccess: (saved) =>
      qc.setQueryData<SongSection[]>(sectionsKey, (old) =>
        (old ?? []).map((s) => (s.id === saved.id ? saved : s)),
      ),
  });
  const deleteMut = useMutation({
    mutationFn: (id: string) => ipc.song.deleteSection(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: sectionsKey });
      // Deletion removes the section from arrangements too.
      void qc.invalidateQueries({ queryKey: ["arrangementItems"] });
      void qc.invalidateQueries({ queryKey: arrangementsKey });
    },
  });
  const reorderMut = useMutation({
    mutationFn: (orderedIds: string[]) => ipc.song.reorderSections(songId, orderedIds),
    onSuccess: (rows) => qc.setQueryData<SongSection[]>(sectionsKey, rows),
  });

  const handleDrop = (to: number) => {
    if (dragFrom === null || dragFrom === to) return setDragFrom(null);
    const ids = sections.map((s) => s.id);
    const [m] = ids.splice(dragFrom, 1);
    ids.splice(to, 0, m);
    setDragFrom(null);
    reorderMut.mutate(ids);
  };

  return (
    <div className="flex flex-col overflow-hidden border-r border-[var(--color-border)]">
      <PanelHeader title="Deler">
        <button
          type="button"
          onClick={() => addMut.mutate()}
          className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <Plus size={13} /> Legg til del
        </button>
      </PanelHeader>

      <div className="flex-1 space-y-3 overflow-y-auto p-4">
        {sections.length === 0 && (
          <p className="text-sm text-[var(--color-fg-muted)]">
            Ingen deler enda. Legg til et vers eller refreng for å starte.
          </p>
        )}
        {sections.map((section, i) => (
          <div
            key={section.id}
            draggable
            onDragStart={() => setDragFrom(i)}
            onDragOver={(e) => e.preventDefault()}
            onDrop={() => handleDrop(i)}
            className={cn(dragFrom === i && "opacity-40")}
          >
            <SectionRow
              section={section}
              onSave={(label, lyrics) => saveMut.mutate({ id: section.id, label, lyrics })}
              onDelete={() => deleteMut.mutate(section.id)}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

function SectionRow({
  section,
  onSave,
  onDelete,
}: {
  section: SongSection;
  onSave: (label: string, lyrics: string) => void;
  onDelete: () => void;
}) {
  const [label, setLabel] = useState(section.label);
  const [lyrics, setLyrics] = useState(section.lyrics);

  const labelOptions = SECTION_LABELS.includes(label) ? SECTION_LABELS : [label, ...SECTION_LABELS];

  return (
    <div className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
      <div className="mb-2 flex items-center gap-2">
        <span className="cursor-grab text-[var(--color-fg-muted)]" title="Dra for å endre rekkefølge">
          ⠿
        </span>
        <select
          value={label}
          onChange={(e) => {
            setLabel(e.target.value);
            onSave(e.target.value, lyrics);
          }}
          className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1 text-xs focus:border-[var(--color-accent)] focus:outline-none"
        >
          {labelOptions.map((l) => (
            <option key={l} value={l}>
              {humanize(l)}
            </option>
          ))}
        </select>
        <span className="text-[10px] text-[var(--color-fg-muted)]">
          {slideCount(lyrics)} lysbilder
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={onDelete}
          title="Slett del"
          className="grid h-6 w-6 place-items-center rounded text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-danger)]"
        >
          <Trash2 size={13} />
        </button>
      </div>
      <textarea
        value={lyrics}
        rows={4}
        placeholder="Tekstlinjer…"
        onChange={(e) => setLyrics(e.target.value)}
        onBlur={() => {
          if (lyrics !== section.lyrics || label !== section.label) onSave(label, lyrics);
        }}
        className="w-full resize-y rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm leading-snug focus:border-[var(--color-accent)] focus:outline-none"
      />
    </div>
  );
}

// ── Arrangement ─────────────────────────────────────────────────────────────────

function ArrangementPanel({
  songId,
  sections,
  arrangements,
  activeArrId,
  onActivate,
  qc,
  arrangementsKey,
}: {
  songId: string;
  sections: SongSection[];
  arrangements: SongArrangement[];
  activeArrId: string | null;
  onActivate: (id: string) => void;
  qc: QC;
  arrangementsKey: readonly unknown[];
}) {
  const itemsKey = ["arrangementItems", activeArrId] as const;
  const itemsQuery = useQuery({
    queryKey: itemsKey,
    queryFn: () => ipc.arrangement.items(activeArrId!),
    enabled: !!activeArrId,
  });

  const [order, setOrder] = useState<string[]>([]);
  useEffect(() => {
    setOrder((itemsQuery.data ?? []).map((i) => i.section_id));
  }, [itemsQuery.data]);

  const sectionMap = useMemo(() => new Map(sections.map((s) => [s.id, s])), [sections]);

  const createMut = useMutation({
    mutationFn: () => ipc.arrangement.create(songId, `Arrangement ${arrangements.length + 1}`),
    onSuccess: (a) => {
      void qc.invalidateQueries({ queryKey: arrangementsKey });
      onActivate(a.id);
    },
  });
  const renameMut = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => ipc.arrangement.rename(id, name),
    onSuccess: () => qc.invalidateQueries({ queryKey: arrangementsKey }),
  });
  const deleteMut = useMutation({
    mutationFn: (id: string) => ipc.arrangement.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: arrangementsKey }),
  });
  const defaultMut = useMutation({
    mutationFn: (id: string) => ipc.arrangement.setDefault(songId, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: arrangementsKey }),
  });
  const duplicateMut = useMutation({
    mutationFn: (id: string) => ipc.arrangement.duplicate(id),
    onSuccess: (a) => {
      void qc.invalidateQueries({ queryKey: arrangementsKey });
      onActivate(a.id);
    },
  });
  const setItemsMut = useMutation({
    mutationFn: (sectionIds: string[]) => ipc.arrangement.setItems(activeArrId!, sectionIds),
    onSuccess: (rows) => qc.setQueryData<ArrangementItem[]>(itemsKey, rows),
  });

  const commitOrder = (next: string[]) => {
    setOrder(next);
    if (activeArrId) setItemsMut.mutate(next);
  };

  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const handleDrop = (to: number) => {
    if (dragFrom === null || dragFrom === to) return setDragFrom(null);
    const next = [...order];
    const [m] = next.splice(dragFrom, 1);
    next.splice(to, 0, m);
    setDragFrom(null);
    commitOrder(next);
  };

  const active = arrangements.find((a) => a.id === activeArrId) ?? null;

  // Generated-slide preview for the active arrangement.
  const generated = useMemo(() => {
    const slides: Array<{ key: string; lines: string[]; label: string }> = [];
    order.forEach((sid, idx) => {
      const sec = sectionMap.get(sid);
      if (!sec) return;
      sectionToSlides(sec.lyrics).forEach((lines, si) => {
        slides.push({ key: `${idx}-${si}`, lines, label: humanize(sec.label) });
      });
    });
    return slides;
  }, [order, sectionMap]);

  return (
    <div className="flex flex-col overflow-hidden">
      <PanelHeader title="Arrangement">
        <select
          value={activeArrId ?? ""}
          onChange={(e) => onActivate(e.target.value)}
          disabled={arrangements.length === 0}
          className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1 text-xs focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-40"
        >
          {arrangements.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
              {Number(a.is_default) === 1 ? " ★" : ""}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={() => createMut.mutate()}
          className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <Plus size={13} /> Ny
        </button>
      </PanelHeader>

      {active ? (
        <div className="flex-1 space-y-4 overflow-y-auto p-4">
          {/* Arrangement actions */}
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => {
                const name = window.prompt("Nytt navn", active.name);
                if (name) renameMut.mutate({ id: active.id, name });
              }}
              className="rounded-md border border-[var(--color-border)] px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
            >
              Gi nytt navn
            </button>
            <button
              type="button"
              onClick={() => defaultMut.mutate(active.id)}
              disabled={Number(active.is_default) === 1}
              className="flex items-center gap-1 rounded-md border border-[var(--color-border)] px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40"
            >
              <Star size={12} /> Standard
            </button>
            <button
              type="button"
              onClick={() => duplicateMut.mutate(active.id)}
              className="flex items-center gap-1 rounded-md border border-[var(--color-border)] px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
            >
              <Copy size={12} /> Dupliser
            </button>
            <button
              type="button"
              onClick={() => deleteMut.mutate(active.id)}
              className="rounded-md border border-[var(--color-border)] px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-danger)]"
            >
              Slett
            </button>
            <span className="flex-1" />
            <span className="text-xs text-[var(--color-fg-muted)]">{generated.length} lysbilder</span>
          </div>

          {/* Section palette → click to append */}
          <div>
            <Subhead>Legg til del i arrangementet</Subhead>
            <div className="flex flex-wrap gap-1.5">
              {sections.length === 0 && (
                <span className="text-xs text-[var(--color-fg-muted)]">Lag deler først.</span>
              )}
              {sections.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  onClick={() => commitOrder([...order, s.id])}
                  className="rounded-full border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2.5 py-1 text-xs hover:border-[var(--color-accent)] hover:text-[var(--color-accent)]"
                >
                  + {humanize(s.label)}
                </button>
              ))}
            </div>
          </div>

          {/* Ordered sequence */}
          <div>
            <Subhead>Rekkefølge</Subhead>
            {order.length === 0 ? (
              <p className="text-xs text-[var(--color-fg-muted)]">
                Tomt arrangement. Klikk en del over for å legge den til.
              </p>
            ) : (
              <ol className="space-y-1">
                {order.map((sid, i) => {
                  const sec = sectionMap.get(sid);
                  return (
                    <li
                      key={`${sid}-${i}`}
                      draggable
                      onDragStart={() => setDragFrom(i)}
                      onDragOver={(e) => e.preventDefault()}
                      onDrop={() => handleDrop(i)}
                      className={cn(
                        "flex items-center gap-2 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-2.5 py-1.5 text-sm",
                        dragFrom === i && "opacity-40",
                      )}
                    >
                      <span className="cursor-grab text-[var(--color-fg-muted)]">⠿</span>
                      <span className="w-5 font-mono text-[10px] text-[var(--color-fg-muted)]">
                        {i + 1}
                      </span>
                      <span className="flex-1">{sec ? humanize(sec.label) : "(slettet)"}</span>
                      <button
                        type="button"
                        onClick={() => commitOrder(order.filter((_, idx) => idx !== i))}
                        className="grid h-5 w-5 place-items-center rounded text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-danger)]"
                      >
                        <X size={12} />
                      </button>
                    </li>
                  );
                })}
              </ol>
            )}
          </div>

          {/* Generated-slide preview */}
          {generated.length > 0 && (
            <div>
              <Subhead>Forhåndsvisning</Subhead>
              <div className="flex gap-2 overflow-x-auto pb-2">
                {generated.map((g) => (
                  <div key={g.key} className="shrink-0">
                    <div className="overflow-hidden rounded-md ring-1 ring-[var(--color-border)]">
                      <SlideCanvas doc={docWithText(g.lines.join("\n"))} width={160} height={90} />
                    </div>
                    <p className="mt-1 text-center text-[10px] text-[var(--color-fg-muted)]">{g.label}</p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="grid flex-1 place-items-center p-4 text-center">
          <div>
            <p className="mb-3 text-sm text-[var(--color-fg-muted)]">Ingen arrangementer enda.</p>
            <button
              type="button"
              onClick={() => createMut.mutate()}
              className="inline-flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110"
            >
              <Plus size={14} /> Lag arrangement
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Shared bits ──────────────────────────────────────────────────────────────────

function PanelHeader({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-4 py-2.5">
      <h2 className="text-sm font-semibold">{title}</h2>
      <div className="flex-1" />
      {children}
    </div>
  );
}

function Subhead({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="mb-1.5 text-[10px] font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
      {children}
    </h3>
  );
}
