/**
 * Operator console — Phase 5.3.
 *
 * Full-screen live control: cue list (current + next), the main-output preview,
 * a "coming next" preview, a notes panel, the next-cues filmstrip, a hotkey
 * legend, and output-health indicators. Authoritative state lives in the Rust
 * `LiveSession` (single dispatcher, persisted for crash recovery); this UI
 * dispatches operator actions and mirrors the returned frame.
 *
 * Output health is a placeholder until the Phase 5.2 output process exists —
 * there are no real displays to report on yet.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronLeft,
  ChevronRight,
  Clapperboard,
  Monitor,
  Search,
  Square,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import type {
  Cue,
  LiveAction,
  LiveFrame,
  LiveSessionView,
  Service,
} from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { StageDisplay } from "./StageDisplay";
import { ExportModal } from "./ExportModal";

interface Props {
  service: Service;
  onExit: () => void;
  /** Attach to an already-recovered session instead of starting a fresh one. */
  resume?: boolean;
}

export function LivePreview({ service, onExit, resume = false }: Props) {
  const [session, setSession] = useState<LiveSessionView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [jumpOpen, setJumpOpen] = useState(false);
  const [stageOpen, setStageOpen] = useState(false);
  const [stagePresetId, setStagePresetId] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

  const cueListQuery = useQuery({
    queryKey: ["cueList", service.id],
    queryFn: () => ipc.live.compileCueList(service.id),
  });
  const cues = useMemo(
    () => cueListQuery.data?.cues ?? [],
    [cueListQuery.data],
  );

  const stagePresetsQuery = useQuery({
    queryKey: ["stagePresets"],
    queryFn: () => ipc.live.stagePresets(),
  });
  const stagePresets = useMemo(
    () => stagePresetsQuery.data ?? [],
    [stagePresetsQuery.data],
  );
  const stagePreset =
    stagePresets.find((p) => p.id === stagePresetId) ?? stagePresets[0];

  // Start a fresh session on mount, or attach to the recovered one if resuming.
  useEffect(() => {
    let cancelled = false;
    const promise = resume ? ipc.live.state() : ipc.live.start(service.id);
    promise
      .then((v) => {
        if (cancelled) return;
        if (v) setSession(v);
        else if (resume) {
          // Recovered session vanished — fall back to a fresh start.
          ipc.live.start(service.id).then((s) => !cancelled && setSession(s));
        }
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [service.id, resume]);

  const exit = useCallback(() => {
    void ipc.live.end().finally(onExit);
  }, [onExit]);

  const dispatch = useCallback((action: LiveAction) => {
    ipc.live
      .dispatch(action)
      .then(setSession)
      .catch((e) => setError(String(e)));
  }, []);

  // Hotkeys
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.target instanceof HTMLInputElement) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        setJumpOpen((o) => !o);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "q") {
        e.preventDefault();
        exit();
        return;
      }
      if (jumpOpen) return;
      switch (e.key) {
        case "ArrowRight":
        case " ":
          e.preventDefault();
          dispatch({ type: "next" });
          break;
        case "ArrowLeft":
          e.preventDefault();
          dispatch({ type: "previous" });
          break;
        case "Escape":
          e.preventDefault();
          dispatch({ type: "blackout" });
          break;
        case "l":
        case "L":
          dispatch({ type: "show_logo" });
          break;
        case "Home":
          dispatch({ type: "go_to", index: 0 });
          break;
        case "End":
          dispatch({ type: "go_to", index: Math.max(0, cues.length - 1) });
          break;
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [dispatch, exit, jumpOpen, cues.length]);

  if (error) {
    return (
      <div className="grid h-full place-items-center bg-[var(--color-bg)]">
        <div className="max-w-md text-center">
          <p className="mb-2 font-semibold text-[var(--color-danger)]">
            Kunne ikke starte live
          </p>
          <p className="text-sm text-[var(--color-fg-muted)]">{error}</p>
          <button
            onClick={exit}
            className="mt-4 rounded-md bg-[var(--color-bg-surface)] px-4 py-2 text-sm hover:brightness-110"
          >
            Avslutt
          </button>
        </div>
      </div>
    );
  }

  if (!session || cueListQuery.isLoading) {
    return (
      <div className="grid h-full place-items-center bg-[var(--color-bg)]">
        <p className="text-sm text-[var(--color-fg-muted)]">Starter live…</p>
      </div>
    );
  }

  if (cues.length === 0) {
    return (
      <div className="grid h-full place-items-center bg-[var(--color-bg)]">
        <div className="max-w-md text-center">
          <p className="mb-1 font-semibold">Ingen cues å vise</p>
          <p className="text-sm text-[var(--color-fg-muted)]">
            «{service.name}» har ingen items enda.
          </p>
          <button
            onClick={exit}
            className="mt-4 rounded-md bg-[var(--color-bg-surface)] px-4 py-2 text-sm hover:brightness-110"
          >
            Tilbake
          </button>
        </div>
      </div>
    );
  }

  const index = session.index;
  const nextCue = cues[index + 1];
  const filmstrip = cues.slice(index + 1, index + 6);

  return (
    <div className="grid h-full grid-cols-[260px_1fr_300px] grid-rows-[1fr_auto] bg-[var(--color-bg)]">
      {/* Left: cue list */}
      <aside className="row-span-2 overflow-y-auto border-r border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
        <div className="mb-3 flex items-center justify-between">
          <div>
            <h2 className="text-sm font-semibold">{service.name}</h2>
            <p className="text-xs text-[var(--color-fg-muted)]">
              {cues.length} cues
            </p>
          </div>
          <button
            type="button"
            onClick={exit}
            title="Avslutt (⌘Q)"
            className="rounded-md p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <Square size={14} fill="currentColor" />
          </button>
        </div>
        <ul className="space-y-0.5">
          {cues.map((cue, i) => (
            <li key={cueId(cue)}>
              <button
                type="button"
                onClick={() => dispatch({ type: "go_to", index: i })}
                className={cn(
                  "flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors",
                  i === index
                    ? "bg-[var(--color-accent)]/15 text-[var(--color-fg)] ring-1 ring-[var(--color-accent)]"
                    : i === index + 1
                      ? "bg-[var(--color-bg-surface)]/60 text-[var(--color-fg)]"
                      : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
                )}
              >
                <span className="w-6 font-mono text-[10px] tabular-nums text-[var(--color-fg-muted)]">
                  {String(i + 1).padStart(3, " ")}
                </span>
                <span className="flex-1 truncate">{cueDisplayLabel(cue)}</span>
                {i === index + 1 && (
                  <span className="text-[9px] uppercase text-[var(--color-fg-muted)]">
                    neste
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      </aside>

      {/* Center: live output */}
      <main className="grid place-items-center overflow-hidden bg-[var(--color-sunday-blue-950)] p-10">
        <div className="aspect-video w-full max-w-4xl overflow-hidden rounded-lg shadow-[0_16px_40px_rgba(0,0,0,0.5)]">
          <FrameRender frame={session.frame} />
        </div>
      </main>

      {/* Right: next + notes */}
      <aside className="row-span-1 flex flex-col gap-3 overflow-y-auto border-l border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-3">
        <div>
          <Subhead>Kommer nå</Subhead>
          <div className="aspect-video overflow-hidden rounded-md ring-1 ring-[var(--color-border)]">
            {nextCue ? (
              <CueMini cue={nextCue} />
            ) : (
              <Empty label="Slutt på listen" />
            )}
          </div>
        </div>

        <div>
          <Subhead>Neste cues</Subhead>
          <div className="space-y-1">
            {filmstrip.length === 0 && (
              <p className="text-xs text-[var(--color-fg-muted)]">—</p>
            )}
            {filmstrip.map((cue, i) => (
              <div
                key={cueId(cue)}
                className="flex items-center gap-2 rounded-md border border-[var(--color-border)] px-2 py-1 text-xs"
              >
                <span className="w-5 font-mono text-[10px] text-[var(--color-fg-muted)]">
                  {index + 2 + i}
                </span>
                <span className="flex-1 truncate text-[var(--color-fg-muted)]">
                  {cueDisplayLabel(cue)}
                </span>
              </div>
            ))}
          </div>
        </div>

        <div className="flex-1">
          <Subhead>Notater</Subhead>
          <p className="whitespace-pre-wrap text-xs text-[var(--color-fg-muted)]">
            {service.notes?.trim()
              ? service.notes
              : "Ingen notater for denne tjenesten."}
          </p>
        </div>
      </aside>

      {/* Bottom strip: controls + health */}
      <footer className="col-start-2 col-end-4 flex items-center gap-4 border-t border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-4 py-2 text-xs text-[var(--color-fg-muted)]">
        <button
          onClick={() => dispatch({ type: "previous" })}
          disabled={index === 0}
          className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)] disabled:opacity-40"
        >
          <ChevronLeft size={14} /> Forrige
        </button>
        <button
          onClick={() => dispatch({ type: "next" })}
          disabled={index >= cues.length - 1}
          className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)] disabled:opacity-40"
        >
          Neste <ChevronRight size={14} />
        </button>
        <button
          onClick={() => dispatch({ type: "blackout" })}
          className={cn(
            "rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]",
            session.output === "blackout" && "text-[var(--color-accent)]",
          )}
        >
          Blackout
        </button>
        <button
          onClick={() => dispatch({ type: "show_logo" })}
          className={cn(
            "rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]",
            session.output === "logo" && "text-[var(--color-accent)]",
          )}
        >
          Logo
        </button>
        <button
          onClick={() => setJumpOpen(true)}
          className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]"
        >
          <Search size={13} /> Hopp til{" "}
          <kbd className="rounded border border-[var(--color-border)] px-1 font-mono text-[10px]">
            ⌘J
          </kbd>
        </button>
        {stagePreset && (
          <button
            onClick={() => setStageOpen(true)}
            className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]"
          >
            <Monitor size={13} /> Sceneskjerm
          </button>
        )}
        <button
          onClick={() => setExportOpen(true)}
          className="flex items-center gap-1 rounded px-2 py-1 hover:bg-[var(--color-bg-surface)]"
        >
          <Clapperboard size={13} /> Eksport
        </button>

        <span className="flex-1" />

        {/* Output health (placeholder until Phase 5.2 output process exists). */}
        <span
          className="flex items-center gap-1.5"
          title="Ingen separat utgangsprosess enda (Phase 5.2)"
        >
          <span className="h-2 w-2 rounded-full bg-[var(--color-warning)]" />
          Ingen utgang tilkoblet
        </span>
        <span>
          Cue <span className="font-mono">{index + 1}</span> /{" "}
          <span className="font-mono">{cues.length}</span>
        </span>
      </footer>

      {jumpOpen && (
        <QuickJump
          cues={cues}
          onPick={(i) => {
            dispatch({ type: "go_to", index: i });
            setJumpOpen(false);
          }}
          onClose={() => setJumpOpen(false)}
        />
      )}

      {stageOpen && stagePreset && (
        <StageDisplay
          session={session}
          cues={cues}
          serviceName={service.name}
          notes={service.notes}
          preset={stagePreset}
          presets={stagePresets}
          onPreset={setStagePresetId}
          onClose={() => setStageOpen(false)}
        />
      )}

      {exportOpen && <ExportModal onClose={() => setExportOpen(false)} />}
    </div>
  );
}

function FrameRender({ frame }: { frame: LiveFrame }) {
  if (frame.kind === "black") {
    return (
      <div className="grid h-full w-full place-items-center bg-black text-xs text-white/30">
        BLACKOUT
      </div>
    );
  }
  if (frame.kind === "logo") {
    return (
      <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-900)] text-2xl font-bold text-[var(--color-accent)]">
        LOGO
      </div>
    );
  }
  if (frame.kind === "message") {
    return (
      <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-900)] text-center text-white/70">
        {frame.text}
      </div>
    );
  }
  const c = frame.slide_content;
  return (
    <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-900)] p-10">
      <div className="w-full max-w-3xl text-center text-white">
        {c.section_label && (
          <div className="mb-5 text-xs font-semibold uppercase tracking-[0.3em] text-[var(--color-accent)]">
            {c.section_label}
          </div>
        )}
        {c.text_lines.map((line, i) => (
          <p
            key={i}
            className="font-semibold leading-tight"
            style={{ fontSize: "var(--text-stage-sm)" }}
          >
            {line}
          </p>
        ))}
        {c.reference && (
          <div className="mt-6 text-base text-white/60">— {c.reference}</div>
        )}
      </div>
    </div>
  );
}

function CueMini({ cue }: { cue: Cue }) {
  if (cue.kind === "show_slide") {
    return (
      <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-900)] p-2 text-center">
        <p className="line-clamp-3 text-[10px] leading-tight text-white">
          {cue.slide_content.text_lines.join(" / ") || cue.source.display_label}
        </p>
      </div>
    );
  }
  return <Empty label={cueDisplayLabel(cue)} />;
}

function Empty({ label }: { label: string }) {
  return (
    <div className="grid h-full w-full place-items-center bg-[var(--color-bg)] text-[10px] text-[var(--color-fg-muted)]">
      {label}
    </div>
  );
}

function QuickJump({
  cues,
  onPick,
  onClose,
}: {
  cues: Cue[];
  onPick: (index: number) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  const matches = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return cues
      .map((cue, i) => ({ cue, i }))
      .filter(
        ({ cue }) =>
          !needle || cueDisplayLabel(cue).toLowerCase().includes(needle),
      )
      .slice(0, 50);
  }, [cues, q]);

  return (
    <div className="fixed inset-0 z-50 grid place-items-start pt-[14vh]">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative mx-auto w-full max-w-xl overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && matches[0]) onPick(matches[0].i);
            if (e.key === "Escape") onClose();
          }}
          placeholder="Hopp til cue… (f.eks. «vers 2» eller «amazing»)"
          className="w-full border-b border-[var(--color-border)] bg-transparent px-4 py-3 text-sm focus:outline-none"
        />
        <ul className="max-h-[50vh] overflow-y-auto p-2">
          {matches.map(({ cue, i }) => (
            <li key={cueId(cue)}>
              <button
                type="button"
                onClick={() => onPick(i)}
                className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm hover:bg-[var(--color-bg-surface)]"
              >
                <span className="w-8 font-mono text-[10px] text-[var(--color-fg-muted)]">
                  {i + 1}
                </span>
                <span className="flex-1 truncate">{cueDisplayLabel(cue)}</span>
              </button>
            </li>
          ))}
          {matches.length === 0 && (
            <li className="px-3 py-6 text-center text-sm text-[var(--color-fg-muted)]">
              Ingen treff.
            </li>
          )}
        </ul>
      </div>
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

function cueId(cue: Cue): string {
  switch (cue.kind) {
    case "show_slide":
      return cue.cue_id;
    case "black_out":
      return cue.cue_id;
    case "show_logo":
      return cue.cue_id;
    case "pause":
      return cue.cue_id;
  }
}

function cueDisplayLabel(cue: Cue): string {
  switch (cue.kind) {
    case "show_slide":
      return cue.source.display_label;
    case "black_out":
      return "Blackout";
    case "show_logo":
      return "Vis logo";
    case "pause":
      return `Pause: ${cue.label}`;
  }
}
