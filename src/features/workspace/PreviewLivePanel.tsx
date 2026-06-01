/**
 * The dual-preview stack every worship console has: PREVIEW (what you're
 * staging) above PROGRAM/LIVE (what's on air right now). Below them, the
 * upcoming cues and the service notes.
 *
 * The key safety property: clicking a slide only changes Preview. Promoting to
 * Live is an explicit "Go" (button, Enter or Space). The Live pane wears an
 * unmistakable gold ● LIVE when the session is armed.
 */
import { BookOpen, Play } from "lucide-react";

import type { Cue, LiveFrame, OutputAppearance } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";
import { SlideView } from "@/components/SlideView";
import { cueDisplayLabel, cueId, frameFromCue, isBibleCue } from "./cueUtils";

interface Props {
  cues: Cue[];
  appearance: OutputAppearance;
  previewIndex: number;
  /** The live frame from the Rust session, or null when not armed. */
  liveFrame: LiveFrame | null;
  liveIndex: number | null;
  isLive: boolean;
  notes: string | null;
  onGo: () => void;
  /** Called when the operator wants to open the bible browser for the current scripture cue. */
  onOpenBibleCue?: () => void;
}

export function PreviewLivePanel({
  cues,
  appearance,
  previewIndex,
  liveFrame,
  liveIndex,
  isLive,
  notes,
  onGo,
  onOpenBibleCue,
}: Props) {
  const t = useT();
  const previewCue = cues[previewIndex] ?? null;
  const previewIsBible = previewCue ? isBibleCue(previewCue) : false;
  const upcoming = cues.slice(
    (liveIndex ?? previewIndex) + 1,
    (liveIndex ?? previewIndex) + 6,
  );

  return (
    <aside className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto border-l border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
      {/* PREVIEW */}
      <div>
        <div className="mb-1.5 flex items-center justify-between gap-1">
          <Subhead>{t("wsPreviewLabel")}</Subhead>
          <div className="flex items-center gap-1">
            {previewIsBible && onOpenBibleCue && (
              <button
                type="button"
                onClick={onOpenBibleCue}
                title={t("wsOpenVerse")}
                className="flex items-center gap-1 rounded-md border border-[var(--color-border)] px-2 py-1 text-[11px] text-[var(--color-fg-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-fg)]"
              >
                <BookOpen size={11} aria-hidden />
                {t("wsOpenVerse")}
              </button>
            )}
            <button
              type="button"
              onClick={onGo}
              disabled={!previewCue}
              title={t("wsGoHint")}
              className="flex items-center gap-1.5 rounded-md bg-[var(--color-accent)] px-2.5 py-1 text-xs font-bold text-[var(--color-sunday-blue-900)] transition-all hover:brightness-110 active:translate-y-px disabled:opacity-40"
            >
              <Play size={12} aria-hidden fill="currentColor" />
              {t("wsGo")}
            </button>
          </div>
        </div>
        <div className="aspect-video overflow-hidden rounded-[var(--radius-md)] bg-[var(--color-stage-black)] ring-1 ring-[var(--color-accent)]/40">
          <SlideView
            frame={previewCue ? frameFromCue(previewCue) : null}
            appearance={appearance}
            forceSectionLabel
            localizeLabel={(l) => localizeSectionLabel(l, t)}
          />
        </div>
        {previewCue && (
          <p className="mt-1 truncate text-[11px] text-[var(--color-fg-muted)]">
            {cueDisplayLabel(previewCue, t)}
          </p>
        )}
      </div>

      {/* PROGRAM / LIVE */}
      <div>
        <div className="mb-1.5 flex items-center justify-between">
          <Subhead>{t("wsProgramLabel")}</Subhead>
          {isLive ? (
            <span className="flex items-center gap-1 rounded bg-[var(--color-on-air)] px-1.5 py-0.5 text-[9px] font-bold text-[var(--color-sunday-blue-900)]">
              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
              {t("wsLiveBadge")}
            </span>
          ) : (
            <span className="text-[10px] text-[var(--color-fg-muted)] uppercase">
              {t("wsNotLive")}
            </span>
          )}
        </div>
        <div
          className={cn(
            "aspect-video overflow-hidden rounded-[var(--radius-md)] bg-[var(--color-stage-black)]",
            isLive
              ? "ring-2 ring-[var(--color-on-air)] shadow-[0_0_0_3px_var(--color-on-air-ring)]"
              : "ring-1 ring-[var(--color-border)]",
          )}
        >
          <SlideView
            frame={isLive ? liveFrame : null}
            appearance={appearance}
            forceSectionLabel
            localizeLabel={(l) => localizeSectionLabel(l, t)}
          />
        </div>
      </div>

      {/* Upcoming cues */}
      <div>
        <Subhead>{t("lpNextCues")}</Subhead>
        <div className="space-y-1">
          {upcoming.length === 0 && (
            <p className="text-xs text-[var(--color-fg-muted)]">—</p>
          )}
          {upcoming.map((cue) => (
            <div
              key={cueId(cue)}
              className="flex items-center gap-2 rounded-md border border-[var(--color-border)] px-2 py-1 text-xs"
            >
              <span className="flex-1 truncate text-[var(--color-fg-muted)]">
                {cueDisplayLabel(cue, t)}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Notes */}
      <div className="flex-1">
        <Subhead>{t("svcNotes")}</Subhead>
        <p className="whitespace-pre-wrap text-xs text-[var(--color-fg-muted)]">
          {notes?.trim() ? notes : t("lpNoNotes")}
        </p>
      </div>
    </aside>
  );
}

function Subhead({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="text-[10px] font-semibold tracking-widest text-[var(--color-fg-muted)] uppercase">
      {children}
    </h3>
  );
}
