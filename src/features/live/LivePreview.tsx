/**
 * Live mode — Phase 5.3 placeholder.
 *
 * Currently a preview: compile a Service's CueList and walk through it
 * with arrow keys. Doesn't yet spawn output processes (Phase 5.2) — that
 * lands when we implement the isolated `sundaystage-output` binary.
 *
 * What this demonstrates:
 *   1. CueList compiler works end-to-end (Service → cues)
 *   2. The text content + section labels render properly
 *   3. Operator hotkeys feel right (arrow keys, Esc, Home, End)
 *   4. The cue list sidebar + main preview layout pattern
 */

import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Square } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { Cue, CueList, Service } from "@/lib/bindings";
import { cn } from "@/lib/cn";

interface Props {
  service: Service;
  onExit: () => void;
}

export function LivePreview({ service, onExit }: Props) {
  const [index, setIndex] = useState(0);
  const [blackout, setBlackout] = useState(false);

  const cueListQuery = useQuery({
    queryKey: ["cueList", service.id],
    queryFn: () => ipc.live.compileCueList(service.id),
  });

  const cueList: CueList | undefined = cueListQuery.data;
  const cues = cueList?.cues ?? [];
  const currentCue = cues[index];

  // Hotkeys
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.target instanceof HTMLInputElement) return;
      switch (e.key) {
        case "ArrowRight":
        case " ":
          e.preventDefault();
          setBlackout(false);
          setIndex((i) => Math.min(cues.length - 1, i + 1));
          break;
        case "ArrowLeft":
          e.preventDefault();
          setBlackout(false);
          setIndex((i) => Math.max(0, i - 1));
          break;
        case "Escape":
          e.preventDefault();
          setBlackout((b) => !b);
          break;
        case "Home":
          setIndex(0);
          setBlackout(false);
          break;
        case "End":
          setIndex(Math.max(0, cues.length - 1));
          setBlackout(false);
          break;
        case "q":
          if (e.metaKey || e.ctrlKey) onExit();
          break;
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [cues.length, onExit]);

  if (cueListQuery.isLoading) {
    return (
      <div className="grid h-full place-items-center bg-[var(--color-bg)]">
        <p className="text-sm text-[var(--color-fg-muted)]">Kompilerer cue-liste…</p>
      </div>
    );
  }

  if (cueListQuery.error) {
    return (
      <div className="grid h-full place-items-center bg-[var(--color-bg)]">
        <div className="text-center max-w-md">
          <p className="font-semibold text-[var(--color-danger)] mb-2">
            Kunne ikke kompilere cue-listen
          </p>
          <p className="text-sm text-[var(--color-fg-muted)]">
            {String(cueListQuery.error)}
          </p>
          <button
            onClick={onExit}
            className="mt-4 rounded-md bg-[var(--color-bg-surface)] px-4 py-2 text-sm hover:brightness-110"
          >
            Avslutt live preview
          </button>
        </div>
      </div>
    );
  }

  if (cues.length === 0) {
    return (
      <div className="grid h-full place-items-center bg-[var(--color-bg)]">
        <div className="text-center max-w-md">
          <p className="font-semibold mb-1">Ingen cues å vise</p>
          <p className="text-sm text-[var(--color-fg-muted)]">
            Tjenesten «{service.name}» har ingen items enda.
          </p>
          <button
            onClick={onExit}
            className="mt-4 rounded-md bg-[var(--color-bg-surface)] px-4 py-2 text-sm hover:brightness-110"
          >
            Tilbake
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="grid h-full grid-cols-[280px_1fr] bg-[var(--color-bg)]">
      {/* Sidebar: cue list */}
      <aside className="overflow-y-auto border-r border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
        <div className="mb-3 flex items-center justify-between">
          <div>
            <h2 className="text-sm font-semibold">{service.name}</h2>
            <p className="text-xs text-[var(--color-fg-muted)]">
              {cues.length} cues
            </p>
          </div>
          <button
            type="button"
            onClick={onExit}
            className="rounded-md p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
            title="Avslutt (⌘Q)"
          >
            <Square size={14} fill="currentColor" />
          </button>
        </div>
        <ul className="space-y-0.5">
          {cues.map((cue, i) => {
            const isActive = i === index;
            const label = cueDisplayLabel(cue);
            return (
              <li key={cueId(cue)}>
                <button
                  type="button"
                  onClick={() => { setIndex(i); setBlackout(false); }}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors",
                    isActive
                      ? "bg-[var(--color-accent)]/15 text-[var(--color-fg)] ring-1 ring-[var(--color-accent)]"
                      : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
                  )}
                >
                  <span className="w-6 font-mono text-[10px] tabular-nums text-[var(--color-fg-muted)]">
                    {String(i + 1).padStart(3, " ")}
                  </span>
                  <span className="flex-1 truncate">{label}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      {/* Main: slide preview */}
      <main className="grid grid-rows-[1fr_auto] overflow-hidden">
        <div
          className={cn(
            "grid place-items-center overflow-hidden p-12",
            blackout ? "bg-black" : "bg-[var(--color-sunday-blue-900)]",
          )}
        >
          {!blackout && currentCue && <SlideRender cue={currentCue} />}
          {blackout && (
            <p className="text-[var(--color-fg-muted)] text-sm">BLACKOUT</p>
          )}
        </div>

        {/* Hotkey strip */}
        <footer className="flex items-center gap-4 border-t border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-4 py-2 text-xs text-[var(--color-fg-muted)]">
          <button
            onClick={() => { setBlackout(false); setIndex((i) => Math.max(0, i - 1)); }}
            disabled={index === 0}
            className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)] disabled:opacity-40"
          >
            <ChevronLeft size={14} /> Forrige
          </button>
          <button
            onClick={() => { setBlackout(false); setIndex((i) => Math.min(cues.length - 1, i + 1)); }}
            disabled={index >= cues.length - 1}
            className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)] disabled:opacity-40"
          >
            Neste <ChevronRight size={14} />
          </button>
          <button
            onClick={() => setBlackout((b) => !b)}
            className={cn(
              "rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]",
              blackout && "text-[var(--color-accent)]",
            )}
          >
            Blackout
          </button>
          <span className="flex-1" />
          <span>
            <kbd className="rounded border border-[var(--color-border)] px-1 font-mono text-[10px]">←</kbd>{" "}
            <kbd className="rounded border border-[var(--color-border)] px-1 font-mono text-[10px]">→</kbd>{" "}
            navigate
          </span>
          <span>
            <kbd className="rounded border border-[var(--color-border)] px-1 font-mono text-[10px]">Esc</kbd>{" "}
            blackout
          </span>
          <span>
            Cue <span className="font-mono">{index + 1}</span> / <span className="font-mono">{cues.length}</span>
          </span>
        </footer>
      </main>
    </div>
  );
}

function SlideRender({ cue }: { cue: Cue }) {
  if (cue.kind !== "show_slide") {
    return (
      <p className="text-[var(--color-fg-muted)] text-sm">
        Cue type: {cue.kind}
      </p>
    );
  }
  const { slide_content } = cue;
  return (
    <div className="text-center text-white max-w-5xl w-full">
      {slide_content.section_label && (
        <div className="mb-6 text-xs uppercase tracking-[0.3em] text-[var(--color-accent)] font-semibold">
          {slide_content.section_label}
        </div>
      )}
      {slide_content.text_lines.map((line, i) => (
        <p
          key={i}
          className="font-semibold leading-tight"
          style={{ fontSize: "var(--text-stage-md)" }}
        >
          {line}
        </p>
      ))}
      {slide_content.reference && (
        <div className="mt-8 text-xl text-white/60 font-medium">
          — {slide_content.reference}
        </div>
      )}
    </div>
  );
}

function cueId(cue: Cue): string {
  switch (cue.kind) {
    case "show_slide": return cue.cue_id;
    case "black_out":  return cue.cue_id;
    case "show_logo":  return cue.cue_id;
    case "pause":      return cue.cue_id;
  }
}

function cueDisplayLabel(cue: Cue): string {
  switch (cue.kind) {
    case "show_slide": return cue.source.display_label;
    case "black_out":  return "Blackout";
    case "show_logo":  return "Vis logo";
    case "pause":      return `Pause: ${cue.label}`;
  }
}
