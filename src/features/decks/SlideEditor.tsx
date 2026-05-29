/**
 * SlideEditor — Phase 3.1.
 *
 * Three-pane deck editor: slide rail (left), canvas (center), inspector
 * (right). Owns the editing document + undo history for the active slide,
 * the block selection, keyboard shortcuts, and debounced autosave to the
 * backend. The canvas itself is dumb — it reports previews/commands up here.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Copy,
  Plus,
  Redo2,
  Trash2,
  Type,
  Undo2,
} from "lucide-react";

import type { CustomDeck, Slide, SlideDoc } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import {
  STAGE_ASPECT,
  blankDoc,
  clampRect,
  docWithText,
  findBlock,
  newBlockId,
  newTextBlock,
  parseDoc,
} from "@/lib/slideEditor/doc";
import {
  addBlockCommand,
  compositeCommand,
  removeBlocksCommand,
  replaceDocCommand,
  updateBlockCommand,
  useEditorHistory,
  type Command,
} from "@/lib/slideEditor/history";
import { useT } from "@/lib/i18n";
import { SlideCanvas } from "./SlideCanvas";
import { Inspector } from "./Inspector";
import { SlideList } from "./SlideList";
import { ThemeControls } from "./ThemeControls";

const NUDGE_X = 1 / 1920;
const NUDGE_Y = 1 / 1080;

interface SlideEditorProps {
  deck: CustomDeck;
  onBack: () => void;
}

export function SlideEditor({ deck, onBack }: SlideEditorProps) {
  const t = useT();
  const qc = useQueryClient();
  const slidesKey = ["slides", deck.id] as const;

  const slidesQuery = useQuery({
    queryKey: slidesKey,
    queryFn: () => ipc.deck.slides(deck.id),
  });
  const slides = useMemo(() => slidesQuery.data ?? [], [slidesQuery.data]);

  const [activeId, setActiveId] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  const history = useEditorHistory(blankDoc());

  // Refs so the (stable) keyboard handler always sees current state.
  const docRef = useRef(history.doc);
  docRef.current = history.doc;
  const selRef = useRef(selectedIds);
  selRef.current = selectedIds;
  const lastSavedRef = useRef<string>("");

  const loadSlide = useCallback(
    (slide: Slide) => {
      const doc = parseDoc(slide.content);
      setActiveId(slide.id);
      setSelectedIds(new Set());
      history.reset(doc);
      lastSavedRef.current = JSON.stringify(doc);
    },
    [history],
  );

  // Select the first slide once data arrives (or when the active one vanishes).
  useEffect(() => {
    if (slides.length === 0) {
      if (activeId !== null) setActiveId(null);
      return;
    }
    if (activeId === null || !slides.some((s) => s.id === activeId)) {
      loadSlide(slides[0]);
    }
  }, [slides, activeId, loadSlide]);

  // ── Autosave (debounced) ───────────────────────────────────────────────────
  const saveMutation = useMutation({
    mutationFn: ({ id, doc }: { id: string; doc: typeof history.doc }) =>
      ipc.deck.updateSlide(id, doc),
    onSuccess: (saved) => {
      qc.setQueryData<Slide[]>(slidesKey, (old) =>
        (old ?? []).map((s) => (s.id === saved.id ? saved : s)),
      );
    },
  });

  useEffect(() => {
    if (!activeId) return;
    const handle = setTimeout(() => {
      const json = JSON.stringify(history.doc);
      if (json !== lastSavedRef.current) {
        lastSavedRef.current = json;
        saveMutation.mutate({ id: activeId, doc: history.doc });
      }
    }, 400);
    return () => clearTimeout(handle);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [history.doc, activeId]);

  // ── Block operations ─────────────────────────────────────────────────────────
  const onSelectBlock = useCallback((id: string | null, additive: boolean) => {
    setSelectedIds((prev) => {
      if (id === null) return new Set();
      if (!additive) return new Set([id]);
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const addText = useCallback(() => {
    const block = newTextBlock();
    history.apply(addBlockCommand(block));
    setSelectedIds(new Set([block.id]));
  }, [history]);

  const deleteSelected = useCallback(() => {
    const ids = [...selRef.current];
    if (ids.length === 0) return;
    history.apply(removeBlocksCommand(docRef.current, ids));
    setSelectedIds(new Set());
  }, [history]);

  const duplicateSelected = useCallback(() => {
    const ids = [...selRef.current];
    if (ids.length === 0) return;
    const newIds: string[] = [];
    const cmds: Command[] = [];
    for (const id of ids) {
      const b = findBlock(docRef.current, id);
      if (!b) continue;
      const copy = {
        ...b,
        id: newBlockId(),
        rect: clampRect({ ...b.rect, x: b.rect.x + 0.02, y: b.rect.y + 0.02 }),
      };
      newIds.push(copy.id);
      cmds.push(addBlockCommand(copy));
    }
    if (cmds.length === 0) return;
    history.apply(
      cmds.length === 1 ? cmds[0] : compositeCommand("Dupliser", cmds),
    );
    setSelectedIds(new Set(newIds));
  }, [history]);

  const nudge = useCallback(
    (dx: number, dy: number) => {
      const ids = [...selRef.current];
      if (ids.length === 0) return;
      const cmds: Command[] = [];
      for (const id of ids) {
        const b = findBlock(docRef.current, id);
        if (!b) continue;
        const after = {
          ...b,
          rect: clampRect({ ...b.rect, x: b.rect.x + dx, y: b.rect.y + dy }),
        };
        cmds.push(updateBlockCommand(b, after));
      }
      if (cmds.length === 0) return;
      history.apply(
        cmds.length === 1 ? cmds[0] : compositeCommand("Flytt", cmds),
      );
    },
    [history],
  );

  // ── Keyboard shortcuts ───────────────────────────────────────────────────────
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const tgt = e.target as HTMLElement | null;
      if (
        tgt &&
        (tgt.tagName === "INPUT" ||
          tgt.tagName === "TEXTAREA" ||
          tgt.tagName === "SELECT")
      ) {
        return;
      }
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) history.redo();
        else history.undo();
        return;
      }
      if (mod && e.key.toLowerCase() === "y") {
        e.preventDefault();
        history.redo();
        return;
      }
      if (mod && e.key.toLowerCase() === "d") {
        e.preventDefault();
        duplicateSelected();
        return;
      }
      switch (e.key) {
        case "Delete":
        case "Backspace":
          e.preventDefault();
          deleteSelected();
          break;
        case "Escape":
          setSelectedIds(new Set());
          break;
        case "ArrowLeft":
          e.preventDefault();
          nudge(-(e.shiftKey ? NUDGE_X * 10 : NUDGE_X), 0);
          break;
        case "ArrowRight":
          e.preventDefault();
          nudge(e.shiftKey ? NUDGE_X * 10 : NUDGE_X, 0);
          break;
        case "ArrowUp":
          e.preventDefault();
          nudge(0, -(e.shiftKey ? NUDGE_Y * 10 : NUDGE_Y));
          break;
        case "ArrowDown":
          e.preventDefault();
          nudge(0, e.shiftKey ? NUDGE_Y * 10 : NUDGE_Y);
          break;
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [history, duplicateSelected, deleteSelected, nudge]);

  // ── Slide operations ─────────────────────────────────────────────────────────
  const addSlideMutation = useMutation({
    mutationFn: () =>
      ipc.deck.createSlide(deck.id, docWithText(t("slideNewContent"))),
    onSuccess: (slide) => {
      qc.setQueryData<Slide[]>(slidesKey, (old) => [...(old ?? []), slide]);
      loadSlide(slide);
    },
  });

  const deleteSlideMutation = useMutation({
    mutationFn: (id: string) => ipc.deck.deleteSlide(id),
    onSuccess: (_v, id) => {
      const remaining = slides.filter((s) => s.id !== id);
      qc.setQueryData<Slide[]>(slidesKey, remaining);
      if (activeId === id) {
        if (remaining.length > 0) loadSlide(remaining[0]);
        else setActiveId(null);
      }
    },
  });

  const reorderMutation = useMutation({
    mutationFn: (orderedIds: string[]) =>
      ipc.deck.reorderSlides(deck.id, orderedIds),
    onSuccess: (reordered) => qc.setQueryData<Slide[]>(slidesKey, reordered),
  });

  // ── Theme / template (Phase 3.2) ──────────────────────────────────────────────
  const activeSlide = activeId
    ? (slides.find((s) => s.id === activeId) ?? null)
    : null;

  const patchSlideInCache = (saved: Slide) =>
    qc.setQueryData<Slide[]>(slidesKey, (old) =>
      (old ?? []).map((s) => (s.id === saved.id ? saved : s)),
    );

  const setSlideThemeMut = useMutation({
    mutationFn: (themeId: string | null) =>
      ipc.deck.setSlideTheme(activeId!, themeId),
    onSuccess: patchSlideInCache,
  });
  const setSlideTemplateMut = useMutation({
    mutationFn: (templateId: string | null) =>
      ipc.deck.setSlideTemplate(activeId!, templateId),
    onSuccess: patchSlideInCache,
  });

  const replaceDoc = useCallback(
    (after: SlideDoc) =>
      history.apply(replaceDocCommand(docRef.current, after)),
    [history],
  );

  // ── Canvas sizing (fit 16:9) ──────────────────────────────────────────────────
  const [wrapRef, wrap] = useElementSize();
  const canvasW = Math.max(
    0,
    Math.floor(Math.min(wrap.w, wrap.h * STAGE_ASPECT)),
  );
  const canvasH = Math.floor(canvasW / STAGE_ASPECT);

  const saveStatus = saveMutation.isPending
    ? t("deckSaving")
    : activeId
      ? t("deckSaved")
      : "";

  return (
    <div className="flex h-full flex-col bg-[var(--color-bg)]">
      {/* Toolbar */}
      <header className="flex items-center gap-2 border-b border-[var(--color-border)] px-3 py-2">
        <button
          type="button"
          onClick={onBack}
          className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <ArrowLeft size={15} /> {t("navDecks")}
        </button>
        <span className="font-semibold">{deck.name}</span>

        <div className="mx-2 h-5 w-px bg-[var(--color-border)]" />

        <ToolbarButton
          onClick={addText}
          icon={<Type size={15} />}
          label={t("deckText")}
        />
        <ToolbarButton
          onClick={duplicateSelected}
          icon={<Copy size={15} />}
          label={t("actionDuplicate")}
          disabled={selectedIds.size === 0}
        />
        <ToolbarButton
          onClick={deleteSelected}
          icon={<Trash2 size={15} />}
          label={t("actionDelete")}
          disabled={selectedIds.size === 0}
        />

        <div className="mx-2 h-5 w-px bg-[var(--color-border)]" />

        <ToolbarButton
          onClick={history.undo}
          icon={<Undo2 size={15} />}
          label={t("deckUndo")}
          disabled={!history.canUndo}
        />
        <ToolbarButton
          onClick={history.redo}
          icon={<Redo2 size={15} />}
          label={t("deckRedo")}
          disabled={!history.canRedo}
        />

        <div className="flex-1" />
        <span className="text-xs text-[var(--color-fg-muted)]">
          {saveStatus}
        </span>
        {activeId && (
          <button
            type="button"
            onClick={() => deleteSlideMutation.mutate(activeId)}
            className="ml-2 rounded-md px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-danger)]"
          >
            {t("deckDeleteSlide")}
          </button>
        )}
      </header>

      {/* Body */}
      <div className="grid flex-1 grid-cols-[200px_1fr_auto] overflow-hidden">
        <SlideList
          slides={slides}
          activeId={activeId}
          onSelect={(id) => {
            const slide = slides.find((s) => s.id === id);
            if (slide) loadSlide(slide);
          }}
          onAdd={() => addSlideMutation.mutate()}
          onReorder={(ids) => reorderMutation.mutate(ids)}
        />

        <div
          ref={wrapRef}
          className="grid place-items-center overflow-hidden bg-[var(--color-bg)] p-8"
        >
          {activeId && canvasW > 0 ? (
            <div className="shadow-[0_16px_40px_rgba(0,0,0,0.4)]">
              <SlideCanvas
                doc={history.doc}
                width={canvasW}
                height={canvasH}
                selectedIds={selectedIds}
                interactive
                onSelect={onSelectBlock}
                onPreview={history.preview}
                onCommit={history.apply}
              />
            </div>
          ) : (
            <EmptyCanvas
              onAdd={() => addSlideMutation.mutate()}
              hasSlides={slides.length > 0}
            />
          )}
        </div>

        <div className="flex w-72 flex-col overflow-y-auto border-l border-[var(--color-border)] bg-[var(--color-bg-elevated)]">
          {activeId && (
            <ThemeControls
              libraryId={deck.library_id}
              doc={history.doc}
              activeSlide={activeSlide}
              onReplaceDoc={replaceDoc}
              onSetSlideTheme={(id) => {
                if (activeId) setSlideThemeMut.mutate(id);
              }}
              onSetSlideTemplate={(id) => {
                if (activeId) setSlideTemplateMut.mutate(id);
              }}
            />
          )}
          <Inspector
            doc={history.doc}
            selectedIds={selectedIds}
            onCommit={history.apply}
            onPreview={history.preview}
          />
        </div>
      </div>
    </div>
  );
}

function ToolbarButton({
  onClick,
  icon,
  label,
  disabled,
}: {
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)] disabled:opacity-40 disabled:hover:bg-transparent"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

function EmptyCanvas({
  onAdd,
  hasSlides,
}: {
  onAdd: () => void;
  hasSlides: boolean;
}) {
  const t = useT();
  return (
    <div className="text-center">
      <p className="mb-3 text-sm text-[var(--color-fg-muted)]">
        {hasSlides ? t("deckSelectSlide") : t("deckNoSlides")}
      </p>
      {!hasSlides && (
        <button
          type="button"
          onClick={onAdd}
          className="inline-flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110"
        >
          <Plus size={15} /> {t("deckAddSlide")}
        </button>
      )}
    </div>
  );
}

function useElementSize() {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const cr = entries[0].contentRect;
      setSize({ w: cr.width, h: cr.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size] as const;
}
