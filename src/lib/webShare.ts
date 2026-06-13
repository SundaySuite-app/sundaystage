/**
 * "Del over nettverk" — push the live frame to SundayStage Web
 * (stage.sundaysuite.app) so phones and extra screens follow along, and
 * subscribe to its remote-control channel so a phone operator can advance the
 * desktop. This is the desktop half of the network-display feature; the web
 * app owns the realtime transport and the display/follow pages.
 *
 * Design mirrors `outputBridge`: a fire-and-forget hook mounted in the
 * operator console. No Rust networking — the renderer already has every
 * LiveFrame, so it forwards over HTTPS directly. Frames are mapped to the
 * web's WebFrame contract with the SAME sensitive-slide gating the Rust
 * companion publisher uses (private content never leaves the building), then
 * coalesced latest-wins with one POST in flight (throttle + ordering) and a
 * monotonic client_seq the server uses to reject stale writes.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import type { LiveFrame, OutputAppearance } from "@/lib/bindings";

/** Default base; overridable from settings for staging/self-host. */
export const DEFAULT_WEB_BASE = "https://stage.sundaysuite.app";
const SENSITIVE_PLACEHOLDER = "Tjeneste pågår";

interface WebFrameAppearance {
  bg_color?: string;
  text_color?: string;
  font_scale?: number;
}

interface WebFrame {
  v: 1;
  kind: "slide" | "black" | "logo" | "message" | "ended";
  text_lines?: string[];
  translation_lines?: string[] | null;
  section_label?: string | null;
  reference?: string | null;
  message?: string;
  appearance?: WebFrameAppearance | null;
}

/** Map a desktop LiveFrame → the web WebFrame, gating sensitive slides. */
export function liveFrameToWebFrame(
  frame: LiveFrame,
  appearance?: OutputAppearance,
): WebFrame {
  const app: WebFrameAppearance | undefined = appearance
    ? {
        bg_color: appearance.bg_color,
        text_color: appearance.text_color,
        font_scale: appearance.text_scale,
      }
    : undefined;

  if (frame.kind === "slide" && frame.slide_content.sensitive_slide) {
    return {
      v: 1,
      kind: "message",
      message: SENSITIVE_PLACEHOLDER,
      appearance: app,
    };
  }
  switch (frame.kind) {
    case "slide":
      return {
        v: 1,
        kind: "slide",
        text_lines: frame.slide_content.text_lines,
        translation_lines: frame.slide_content.translation_lines,
        section_label: frame.slide_content.section_label,
        reference: frame.slide_content.reference,
        appearance: app,
      };
    case "message":
      return { v: 1, kind: "message", message: frame.text, appearance: app };
    case "black":
      return { v: 1, kind: "black", appearance: app };
    case "logo":
      return { v: 1, kind: "logo", appearance: app };
  }
}

export interface WebShareSession {
  id: string;
  code: string;
  secret: string;
}

export type ShareStatus = "off" | "starting" | "sharing" | "error";

/** A remote command from the web operator (the desktop maps these to actions). */
export type RemoteCommand = "next" | "prev" | "black" | "logo" | "clear";

export interface WebShareController {
  status: ShareStatus;
  session: WebShareSession | null;
  error: string | null;
  start: () => Promise<void>;
  stop: () => Promise<void>;
}

/**
 * Drive a web-share session: `start()` creates a session on the web app
 * (desktop origin), then every frame change is forwarded; `onCommand` fires
 * for remote-control commands arriving on the session's command channel
 * (wired by the caller to the live dispatcher). Forwarding stops when `active`
 * goes false or `stop()` is called.
 */
export function useWebShare(opts: {
  frame: LiveFrame | null;
  appearance?: OutputAppearance;
  active: boolean;
  baseUrl?: string;
  onCommand?: (cmd: RemoteCommand) => void;
}): WebShareController {
  const { frame, appearance, active, onCommand } = opts;
  const baseUrl = (opts.baseUrl ?? DEFAULT_WEB_BASE).replace(/\/$/, "");

  const [status, setStatus] = useState<ShareStatus>("off");
  const [session, setSession] = useState<WebShareSession | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Latest-wins coalescer state (refs so re-renders don't reset it).
  const pending = useRef<WebFrame | null>(null);
  const inFlight = useRef(false);
  const clientSeq = useRef(0);
  const onCommandRef = useRef(onCommand);
  useEffect(() => {
    onCommandRef.current = onCommand;
  });

  const drain = useCallback(
    async (sess: WebShareSession) => {
      if (inFlight.current) return;
      while (pending.current !== null) {
        const webFrame = pending.current;
        pending.current = null;
        inFlight.current = true;
        clientSeq.current += 1;
        try {
          const res = await fetch(`${baseUrl}/api/sessions/${sess.id}/frame`, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              Authorization: `Bearer ${sess.secret}`,
            },
            body: JSON.stringify({
              frame: webFrame,
              client_seq: clientSeq.current,
            }),
          });
          setStatus(res.ok ? "sharing" : "error");
          if (!res.ok) setError(`HTTP ${res.status}`);
        } catch (e) {
          setStatus("error");
          setError(e instanceof Error ? e.message : String(e));
        } finally {
          inFlight.current = false;
        }
      }
    },
    [baseUrl],
  );

  const start = useCallback(async () => {
    setStatus("starting");
    setError(null);
    try {
      const res = await fetch(`${baseUrl}/api/sessions`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ origin: "desktop", title: "SundayStage" }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const sess = (await res.json()) as WebShareSession;
      setSession(sess);
      setStatus("sharing");
    } catch (e) {
      setStatus("error");
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [baseUrl]);

  const stop = useCallback(async () => {
    const sess = session;
    setSession(null);
    setStatus("off");
    if (sess) {
      await fetch(`${baseUrl}/api/sessions/${sess.id}/end`, {
        method: "POST",
        headers: { Authorization: `Bearer ${sess.secret}` },
      }).catch(() => {});
    }
  }, [baseUrl, session]);

  // Forward frames while sharing + active.
  useEffect(() => {
    if (!session || !active || !frame) return;
    pending.current = liveFrameToWebFrame(frame, appearance);
    void drain(session);
  }, [frame, appearance, active, session, drain]);

  // Subscribe to the remote-control channel (the smart desktop integration).
  useEffect(() => {
    if (!session) return;
    let cancelled = false;
    let cleanup: (() => void) | undefined;
    void (async () => {
      try {
        const { createClient } = await import("@supabase/supabase-js");
        const url = import.meta.env.VITE_SUNDAY_SUPABASE_URL as
          | string
          | undefined;
        const anon = import.meta.env.VITE_SUNDAY_SUPABASE_ANON_KEY as
          | string
          | undefined;
        if (!url || !anon) return; // remote control needs the public Supabase creds
        const supabase = createClient(url, anon);
        let lastCmdSeq = 0;
        const channel = supabase.channel(
          `stage:session:${session.id}:commands`,
        );
        channel.on("broadcast", { event: "command" }, (msg) => {
          const payload = (msg.payload ?? {}) as {
            cmd?: RemoteCommand;
            cmd_seq?: number;
          };
          const seq = payload.cmd_seq ?? 0;
          if (seq <= lastCmdSeq || !payload.cmd) return; // replay / stale guard
          lastCmdSeq = seq;
          onCommandRef.current?.(payload.cmd);
        });
        channel.subscribe();
        if (cancelled) {
          void supabase.removeChannel(channel);
          return;
        }
        cleanup = () => void supabase.removeChannel(channel);
      } catch {
        // remote control is optional; forwarding still works without it
      }
    })();
    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [session]);

  return { status, session, error, start, stop };
}
