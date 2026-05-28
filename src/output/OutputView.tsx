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
import type { LiveFrame } from "@/lib/bindings";
import { OUTPUT_RENDER, OUTPUT_HEARTBEAT } from "@/lib/outputBridge";

const TIMEOUT_MS = 2000;

type Role = "main" | "stage" | "confidence";

function roleFromLabel(label: string): Role {
  if (label.includes("-stage-")) return "stage";
  if (label.includes("-confidence-")) return "confidence";
  return "main";
}

export function OutputView() {
  const [frame, setFrame] = useState<LiveFrame | null>(null);
  const [disconnected, setDisconnected] = useState(false);
  const [role, setRole] = useState<Role>("main");
  const lastBeat = useRef<number>(Date.now());
  const lastSeq = useRef<number>(0);

  useEffect(() => {
    try {
      setRole(roleFromLabel(getCurrentWebviewWindow().label));
    } catch {
      /* not running inside Tauri */
    }
  }, []);

  // Late-join: the window may open mid-service — pull the current frame once.
  useEffect(() => {
    ipc.live
      .state()
      .then((v) => v && setFrame(v.frame))
      .catch(() => {});
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
      <OutputFrame frame={frame} />
      {chrome && <Clock />}
      {disconnected && chrome && (
        <div className="absolute top-4 right-4 rounded-md bg-[var(--color-warning)] px-3 py-1.5 text-sm font-semibold text-black shadow-lg">
          Mistet forbindelse — holder siste bilde
        </div>
      )}
    </div>
  );
}

function OutputFrame({ frame }: { frame: LiveFrame | null }) {
  if (!frame || frame.kind === "black") {
    return <div className="h-full w-full bg-black" />;
  }
  if (frame.kind === "logo") {
    return (
      <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-950)] font-bold text-[var(--color-accent)] [font-size:8vw]">
        SundayStage
      </div>
    );
  }
  if (frame.kind === "message") {
    return (
      <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-950)] px-[8vw] text-center [font-size:4vw]">
        {frame.text}
      </div>
    );
  }
  const c = frame.slide_content;
  return (
    <div className="grid h-full w-full place-items-center bg-[var(--color-sunday-blue-950)] px-[6vw] text-center">
      <div className="w-full">
        {c.section_label && (
          <div className="mb-[3vh] font-semibold tracking-[0.3em] text-[var(--color-accent)] uppercase [font-size:1.6vw]">
            {c.section_label}
          </div>
        )}
        {c.text_lines.map((line, i) => (
          <p key={i} className="font-semibold leading-tight [font-size:5.5vw]">
            {line}
          </p>
        ))}
        {c.translation_lines &&
          c.translation_lines.map((line, i) => (
            <p
              key={`t-${i}`}
              className="mt-[1vh] text-white/70 [font-size:3.2vw]"
            >
              {line}
            </p>
          ))}
        {c.reference && (
          <div className="mt-[4vh] text-white/60 [font-size:2vw]">
            — {c.reference}
          </div>
        )}
      </div>
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
