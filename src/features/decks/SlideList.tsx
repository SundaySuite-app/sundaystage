/**
 * SlideList — Phase 3.1 left rail.
 *
 * Vertical slide thumbnails (rendered through the same SlideCanvas as the
 * editor, so a thumbnail is a true miniature of the output). Click to select,
 * drag to reorder. Reordering is committed to the backend by the parent.
 */

import { useState } from "react";
import { Plus } from "lucide-react";

import type { Slide } from "@/lib/bindings";
import { parseDoc } from "@/lib/slideEditor/doc";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { SlideCanvas } from "./SlideCanvas";

const THUMB_W = 168;
const THUMB_H = Math.round((THUMB_W * 9) / 16);

interface SlideListProps {
  slides: Slide[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onReorder: (orderedIds: string[]) => void;
}

export function SlideList({
  slides,
  activeId,
  onSelect,
  onAdd,
  onReorder,
}: SlideListProps) {
  const t = useT();
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  const handleDrop = (to: number) => {
    if (dragFrom === null || dragFrom === to) {
      setDragFrom(null);
      setDragOver(null);
      return;
    }
    const ids = slides.map((s) => s.id);
    const [moved] = ids.splice(dragFrom, 1);
    ids.splice(to, 0, moved);
    setDragFrom(null);
    setDragOver(null);
    onReorder(ids);
  };

  return (
    <div className="flex h-full w-[200px] flex-col border-r border-[var(--color-border)] bg-[var(--color-bg-elevated)]">
      <div className="flex items-center justify-between px-3 py-2.5">
        <span className="text-xs font-semibold text-[var(--color-fg-muted)]">
          {t(slides.length === 1 ? "slideCountOne" : "slideCountMany", {
            n: slides.length,
          })}
        </span>
        <button
          type="button"
          onClick={onAdd}
          title={t("slideNew")}
          className="grid h-6 w-6 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <Plus size={15} />
        </button>
      </div>

      <ol className="flex-1 space-y-2 overflow-y-auto px-3 pb-4">
        {slides.map((slide, i) => {
          const doc = parseDoc(slide.content);
          const isActive = slide.id === activeId;
          return (
            <li
              key={slide.id}
              draggable
              onDragStart={() => setDragFrom(i)}
              onDragEnter={() => setDragOver(i)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => handleDrop(i)}
              onDragEnd={() => {
                setDragFrom(null);
                setDragOver(null);
              }}
              className={cn(
                "group flex cursor-pointer items-start gap-2 rounded-md p-1 transition-colors",
                dragOver === i && dragFrom !== null && dragFrom !== i
                  ? "ring-1 ring-[var(--color-accent)]"
                  : "",
                dragFrom === i ? "opacity-40" : "",
              )}
              onClick={() => onSelect(slide.id)}
            >
              <span className="w-4 pt-1 text-right font-mono text-[10px] tabular-nums text-[var(--color-fg-muted)]">
                {i + 1}
              </span>
              <div
                className={cn(
                  "overflow-hidden rounded-md ring-1 transition-shadow",
                  isActive
                    ? "ring-2 ring-[var(--color-accent)]"
                    : "ring-[var(--color-border)]",
                )}
              >
                <SlideCanvas doc={doc} width={THUMB_W} height={THUMB_H} />
              </div>
            </li>
          );
        })}
        {slides.length === 0 && (
          <li className="px-1 py-6 text-center text-xs text-[var(--color-fg-muted)]">
            {t("slideListEmpty")}
          </li>
        )}
      </ol>
    </div>
  );
}
