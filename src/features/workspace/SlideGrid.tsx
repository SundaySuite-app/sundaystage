/**
 * The heart of the operator workspace — the ProPresenter/FreeShow slide grid.
 *
 * The selected service compiles to a flat cue list; we render every cue as a
 * 16:9 thumbnail (the *same* `SlideView` the projector uses), grouped by
 * service item with a small heading so the operator can see the shape of the
 * service at a glance. Clicking a thumbnail *stages* it in Preview — it does
 * NOT go on air. Gold marks the staged slide; a solid gold ring + ● marks
 * what is live right now.
 */
import { useEffect, useRef } from "react";

import type { Cue, OutputAppearance } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";
import { SlideView } from "@/components/SlideView";
import { cueId, cueServiceItemId, frameFromCue } from "./cueUtils";

interface Props {
  cues: Cue[];
  appearance: OutputAppearance;
  /** Staged slide (local preview). */
  previewIndex: number;
  /** Index that is currently on air, or null when not live. */
  liveIndex: number | null;
  /** service_item_id → human title, for group headings. */
  itemTitles: Map<string, string>;
  onPreview: (index: number) => void;
}

export function SlideGrid({
  cues,
  appearance,
  previewIndex,
  liveIndex,
  itemTitles,
  onPreview,
}: Props) {
  const t = useT();
  const previewRef = useRef<HTMLButtonElement>(null);

  // Keep the staged slide in view as the operator arrows through the service.
  useEffect(() => {
    previewRef.current?.scrollIntoView({ block: "nearest" });
  }, [previewIndex]);

  if (cues.length === 0) {
    return (
      <div className="grid h-full place-items-center p-10 text-center">
        <div className="max-w-sm">
          <h3 className="text-[var(--text-ui-lg)] font-semibold">
            {t("wsSlidesEmptyTitle")}
          </h3>
          <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
            {t("wsSlidesEmptyBody")}
          </p>
        </div>
      </div>
    );
  }

  // Walk the flat list, emitting a heading row whenever the owning service item
  // changes so the grid reads as "Song 1 · Scripture · Song 2 · …".
  const rows: Array<
    | { type: "heading"; key: string; label: string }
    | { type: "slide"; key: string; cue: Cue; index: number }
  > = [];
  let lastItem: string | null | undefined;
  cues.forEach((cue, index) => {
    const itemId = cueServiceItemId(cue);
    if (itemId !== lastItem) {
      lastItem = itemId;
      const label = itemId ? (itemTitles.get(itemId) ?? "") : "";
      if (label) rows.push({ type: "heading", key: `h-${index}`, label });
    }
    rows.push({ type: "slide", key: cueId(cue), cue, index });
  });

  return (
    <div className="h-full overflow-y-auto p-4">
      <GroupedGrid
        rows={rows}
        appearance={appearance}
        previewIndex={previewIndex}
        liveIndex={liveIndex}
        previewRef={previewRef}
        onPreview={onPreview}
      />
    </div>
  );
}

/** Lays out the walked rows: each heading starts a new responsive slide grid. */
function GroupedGrid({
  rows,
  appearance,
  previewIndex,
  liveIndex,
  previewRef,
  onPreview,
}: {
  rows: Array<
    | { type: "heading"; key: string; label: string }
    | { type: "slide"; key: string; cue: Cue; index: number }
  >;
  appearance: OutputAppearance;
  previewIndex: number;
  liveIndex: number | null;
  previewRef: React.RefObject<HTMLButtonElement | null>;
  onPreview: (index: number) => void;
}) {
  const t = useT();
  // Re-group into [{heading, slides[]}] segments.
  const segments: Array<{
    key: string;
    label: string | null;
    slides: Array<{ key: string; cue: Cue; index: number }>;
  }> = [];
  for (const row of rows) {
    if (row.type === "heading") {
      segments.push({ key: row.key, label: row.label, slides: [] });
    } else {
      if (segments.length === 0)
        segments.push({ key: "seg-0", label: null, slides: [] });
      segments[segments.length - 1].slides.push(row);
    }
  }

  return (
    <div className="flex flex-col gap-5">
      {segments.map((seg) => (
        <section key={seg.key}>
          {seg.label && (
            <h3 className="mb-2 px-0.5 text-[11px] font-semibold tracking-widest text-[var(--color-fg-muted)] uppercase">
              {seg.label}
            </h3>
          )}
          <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3">
            {seg.slides.map(({ key, cue, index }) => {
              const isPreview = index === previewIndex;
              const isLive = liveIndex === index;
              const sectionLabel =
                cue.kind === "show_slide"
                  ? cue.slide_content.section_label
                  : null;
              return (
                <button
                  key={key}
                  ref={isPreview ? previewRef : undefined}
                  type="button"
                  onClick={() => onPreview(index)}
                  title={t("wsStageThisSlide")}
                  className={cn(
                    "group relative overflow-hidden rounded-[var(--radius-md)] text-left transition-shadow",
                    "ring-1 ring-[var(--color-border)] hover:ring-[var(--color-fg-muted)]",
                    isPreview &&
                      "ring-2 ring-[var(--color-accent)] ring-offset-2 ring-offset-[var(--color-bg)]",
                    isLive &&
                      "ring-2 ring-[var(--color-on-air)] shadow-[0_0_0_2px_var(--color-on-air-ring)]",
                  )}
                >
                  <span className="absolute top-1 left-1 z-10 rounded bg-black/55 px-1.5 py-0.5 font-mono text-[10px] text-white tabular-nums">
                    {index + 1}
                  </span>
                  {isLive && (
                    <span className="absolute top-1 right-1 z-10 flex items-center gap-1 rounded bg-[var(--color-on-air)] px-1.5 py-0.5 text-[9px] font-bold text-[var(--color-sunday-blue-900)]">
                      <span className="h-1.5 w-1.5 rounded-full bg-current" />
                      {t("wsLiveBadge")}
                    </span>
                  )}
                  <div className="aspect-video w-full bg-[var(--color-stage-black)]">
                    <SlideView
                      frame={frameFromCue(cue)}
                      appearance={appearance}
                      forceSectionLabel
                      localizeLabel={(l) => localizeSectionLabel(l, t)}
                    />
                  </div>
                  {sectionLabel && (
                    <span className="block truncate border-t border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-2 py-1 text-[11px] text-[var(--color-fg-muted)]">
                      {localizeSectionLabel(sectionLabel, t)}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}
