/**
 * Stage display — Phase 8.
 *
 * The screen the worship leader and musicians watch: current lyrics big, the
 * next cue, the section label (musicians steer by it), a clock + service timer,
 * and notes — each panel gated by the chosen preset. Driven by the same live
 * session as the operator console; in production this renders in a separate
 * window on its own screen (Phase 5.2), here it's a full-screen in-app view.
 */

import { useEffect, useState } from "react";
import { X } from "lucide-react";

import type { Cue, LiveSessionView, StageDisplayConfig } from "@/lib/bindings";
import { useT, useLocale } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";

function fmtDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${m}:${ss}`;
}

function cueText(cue: Cue | undefined): {
  lines: string[];
  section: string | null;
} {
  if (!cue) return { lines: [], section: null };
  if (cue.kind === "show_slide") {
    return {
      lines: cue.slide_content.text_lines,
      section: cue.slide_content.section_label,
    };
  }
  if (cue.kind === "pause") return { lines: [cue.label], section: null };
  return { lines: [], section: null };
}

interface StageDisplayProps {
  session: LiveSessionView;
  cues: Cue[];
  serviceName: string;
  notes: string | null;
  preset: StageDisplayConfig;
  presets: StageDisplayConfig[];
  onPreset: (id: string) => void;
  onClose: () => void;
}

export function StageDisplay({
  session,
  cues,
  serviceName,
  notes,
  preset,
  presets,
  onPreset,
  onClose,
}: StageDisplayProps) {
  const t = useT();
  const lang = useLocale((s) => s.lang);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const current =
    session.frame.kind === "slide"
      ? {
          lines: session.frame.slide_content.text_lines,
          section: session.frame.slide_content.section_label,
        }
      : session.frame.kind === "message"
        ? { lines: [session.frame.text], section: null }
        : { lines: [], section: null };
  const next = cueText(cues[session.index + 1]);

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-black text-white">
      {/* Top bar */}
      <header className="flex items-center gap-4 border-b border-white/10 px-6 py-3">
        <span className="text-sm text-white/50">{serviceName}</span>
        {preset.show_section_label && current.section && (
          <span className="rounded-full bg-[var(--color-accent)] px-3 py-1 text-sm font-bold text-black">
            {localizeSectionLabel(current.section, t)}
          </span>
        )}
        <div className="flex-1" />
        {preset.show_service_timer && (
          <span
            className="font-mono text-lg tabular-nums text-white/70"
            title={t("sdTimeSinceStart")}
          >
            ⏱ {fmtDuration(now - Number(session.started_at))}
          </span>
        )}
        {preset.show_clock && (
          <span className="font-mono text-lg tabular-nums text-white/90">
            {new Date(now).toLocaleTimeString(lang, {
              hour: "2-digit",
              minute: "2-digit",
            })}
          </span>
        )}
        <select
          value={preset.id}
          onChange={(e) => onPreset(e.target.value)}
          className="rounded-md border border-white/20 bg-white/5 px-2 py-1 text-xs focus:outline-none"
        >
          {presets.map((p) => (
            <option key={p.id} value={p.id} className="text-black">
              {p.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={onClose}
          title={t("sdCloseEsc")}
          className="grid h-8 w-8 place-items-center rounded-md text-white/60 hover:bg-white/10 hover:text-white"
        >
          <X size={18} />
        </button>
      </header>

      {/* Body */}
      <div className="grid flex-1 grid-cols-[1fr_auto] overflow-hidden">
        <main className="grid place-items-center overflow-hidden p-12 text-center">
          {session.frame.kind === "black" ? (
            <p className="text-2xl text-white/30">BLACKOUT</p>
          ) : current.lines.length > 0 ? (
            <div>
              {current.lines.map((line, i) => (
                <p
                  key={i}
                  className="font-bold leading-tight"
                  style={{
                    fontSize: preset.lyrics_large
                      ? "var(--text-stage-lg)"
                      : "var(--text-stage-md)",
                  }}
                >
                  {line}
                </p>
              ))}
            </div>
          ) : (
            <p className="text-2xl text-white/30">—</p>
          )}
        </main>

        {(preset.show_next_slide || preset.show_notes) && (
          <aside className="flex w-80 flex-col gap-4 border-l border-white/10 p-5">
            {preset.show_next_slide && (
              <div>
                <h3 className="mb-1 text-xs font-semibold uppercase tracking-widest text-white/40">
                  {t("actionNext")}
                </h3>
                {next.section && (
                  <p className="text-sm font-bold text-[var(--color-accent)]">
                    {localizeSectionLabel(next.section, t)}
                  </p>
                )}
                <p className="line-clamp-4 text-lg leading-snug text-white/70">
                  {next.lines.join(" / ") || t("liveEndOfList")}
                </p>
              </div>
            )}
            {preset.show_notes && (
              <div className="flex-1">
                <h3 className="mb-1 text-xs font-semibold uppercase tracking-widest text-white/40">
                  {t("svcNotes")}
                </h3>
                <p className="whitespace-pre-wrap text-sm text-white/60">
                  {notes?.trim() ? notes : t("sdNoNotes")}
                </p>
              </div>
            )}
          </aside>
        )}
      </div>

      <footer className="border-t border-white/10 px-6 py-2 text-center text-xs text-white/30">
        Cue {session.index + 1} / {session.total} · {t("liveStageScreen")}:{" "}
        {preset.name}
      </footer>
    </div>
  );
}
