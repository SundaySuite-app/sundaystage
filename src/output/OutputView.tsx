/**
 * Phase 5.2 — the renderer that runs in each borderless full-screen output
 * window (loaded from output.html). It subscribes to the operator UI's render +
 * heartbeat events and paints the current frame. Its watchdog mirrors the Rust
 * `output::Watchdog`: if the heartbeat stops, hold the last frame — never blank
 * the congregation. A lost-connection badge shows on stage/confidence screens
 * only (the main output stays clean).
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import { ipc } from "@/lib/ipc";
import type { LiveFrame, OutputAppearance } from "@/lib/bindings";
import {
  OUTPUT_RENDER,
  OUTPUT_HEARTBEAT,
  OUTPUT_APPEARANCE,
  DEFAULT_OUTPUT_APPEARANCE,
} from "@/lib/outputBridge";
import { SlideView } from "@/components/SlideView";
import { useT } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";

const TIMEOUT_MS = 2000;

type Role = "main" | "stage" | "confidence";

function roleFromLabel(label: string): Role {
  if (label.includes("-stage-")) return "stage";
  if (label.includes("-confidence-")) return "confidence";
  return "main";
}

export function OutputView() {
  const [frame, setFrame] = useState<LiveFrame | null>(null);
  const [appearance, setAppearance] = useState<OutputAppearance>(
    DEFAULT_OUTPUT_APPEARANCE,
  );
  const [disconnected, setDisconnected] = useState(false);
  const [role, setRole] = useState<Role>("main");
  const lastBeat = useRef<number>(Date.now());
  const lastSeq = useRef<number>(0);
  const t = useT();

  useEffect(() => {
    try {
      setRole(roleFromLabel(getCurrentWebviewWindow().label));
    } catch {
      /* not running inside Tauri */
    }
  }, []);

  // Late-join: the window may open mid-service — pull the current frame once,
  // plus the saved appearance.
  useEffect(() => {
    ipc.live
      .state()
      .then((v) => v && setFrame(v.frame))
      .catch(() => {});
    ipc.output
      .appearance()
      .then(setAppearance)
      .catch(() => {});
  }, []);

  // Restyle live when the operator changes appearance in Settings.
  useEffect(() => {
    let un: (() => void) | undefined;
    void listen<OutputAppearance>(OUTPUT_APPEARANCE, (e) =>
      setAppearance(e.payload),
    ).then((u) => (un = u));
    return () => un?.();
  }, []);

  useEffect(() => {
    const unlisten: Array<() => void> = [];
    void listen<{ frame: LiveFrame; seq: number }>(OUTPUT_RENDER, (e) => {
      if (e.payload.seq < lastSeq.current) return; // drop stale renders
      lastSeq.current = e.payload.seq;
      lastBeat.current = Date.now();
      setDisconnected(false);
      setFrame(e.payload.frame);
    }).then((u) => unlisten.push(u));
    void listen(OUTPUT_HEARTBEAT, () => {
      lastBeat.current = Date.now();
      setDisconnected(false);
    }).then((u) => unlisten.push(u));
    return () => unlisten.forEach((u) => u());
  }, []);

  // Watchdog — hold the last frame when the operator UI goes quiet.
  useEffect(() => {
    const id = setInterval(() => {
      if (Date.now() - lastBeat.current > TIMEOUT_MS) setDisconnected(true);
    }, 500);
    return () => clearInterval(id);
  }, []);

  const chrome = role !== "main";

  return (
    <div className="relative h-screen w-screen overflow-hidden bg-black text-white">
      <SlideView
        frame={frame}
        appearance={appearance}
        forceSectionLabel={chrome}
        localizeLabel={(l) => localizeSectionLabel(l, t)}
      />
      {chrome && <Clock />}
      {disconnected && chrome && (
        <div className="absolute top-4 right-4 rounded-md bg-[var(--color-warning)] px-3 py-1.5 text-sm font-semibold text-black shadow-lg">
          Mistet forbindelse — holder siste bilde
        </div>
      )}
    </div>
  );
}

function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);
  const hh = String(now.getHours()).padStart(2, "0");
  const mm = String(now.getMinutes()).padStart(2, "0");
  return (
    <div className="absolute bottom-4 left-4 font-mono text-white/50 [font-size:2vw]">
      {hh}:{mm}
    </div>
  );
}
