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
  /** The NEXT slide's content, for the web scene/confidence monitor (/s). */
  next_lines?: string[];
  next_label?: string | null;
  section_label?: string | null;
  reference?: string | null;
  message?: string;
  appearance?: WebFrameAppearance | null;
}

/** The next slide's content, threaded through for the scene monitor. */
export interface NextSlideInfo {
  lines: string[];
  label?: string | null;
}

// ── Contract limits ──────────────────────────────────────────────────────────
// Mirror the web app's zod schema (sundaystage-web/lib/webframe.ts). The
// server rejects the WHOLE frame on any out-of-range field — which silently
// freezes every display on the previous slide — so we clamp/truncate here and
// a frame is never rejected wholesale. Pinned by webShare.test.ts against the
// vendored webframe.schema.json.
const MAX_LINES = 40;
const MAX_LINE_CHARS = 500;
const MAX_LABEL_CHARS = 80;
const MAX_REFERENCE_CHARS = 120;
const MAX_MESSAGE_CHARS = 2000;
const MAX_COLOR_CHARS = 32;
const FONT_SCALE_MIN = 0.3;
const FONT_SCALE_MAX = 3;

const truncate = (s: string, max: number) =>
  s.length > max ? s.slice(0, max) : s;
const clampLines = (lines: string[]) =>
  lines.slice(0, MAX_LINES).map((l) => truncate(l, MAX_LINE_CHARS));
const clampLabel = (s: string | null | undefined) =>
  s == null ? s : truncate(s, MAX_LABEL_CHARS);

function clampAppearance(
  appearance?: OutputAppearance,
): WebFrameAppearance | undefined {
  if (!appearance) return undefined;
  const scale = appearance.text_scale;
  return {
    bg_color: truncate(appearance.bg_color, MAX_COLOR_CHARS),
    text_color: truncate(appearance.text_color, MAX_COLOR_CHARS),
    font_scale: Number.isFinite(scale)
      ? Math.min(FONT_SCALE_MAX, Math.max(FONT_SCALE_MIN, scale))
      : 1,
  };
}

/** Map a desktop LiveFrame → the web WebFrame, gating sensitive slides. */
export function liveFrameToWebFrame(
  frame: LiveFrame,
  appearance?: OutputAppearance,
  next?: NextSlideInfo | null,
): WebFrame {
  const app = clampAppearance(appearance);

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
        text_lines: clampLines(frame.slide_content.text_lines),
        translation_lines: frame.slide_content.translation_lines
          ? clampLines(frame.slide_content.translation_lines)
          : frame.slide_content.translation_lines,
        next_lines: next ? clampLines(next.lines) : undefined,
        next_label: next ? clampLabel(next.label) : undefined,
        section_label: clampLabel(frame.slide_content.section_label),
        reference:
          frame.slide_content.reference == null
            ? frame.slide_content.reference
            : truncate(frame.slide_content.reference, MAX_REFERENCE_CHARS),
        appearance: app,
      };
    case "message":
      return {
        v: 1,
        kind: "message",
        message: truncate(frame.text, MAX_MESSAGE_CHARS),
        appearance: app,
      };
    case "black":
      return { v: 1, kind: "black", appearance: app };
    case "logo":
      return { v: 1, kind: "logo", appearance: app };
  }
}

// ── Remote-command authentication ────────────────────────────────────────────
// The commands channel is reachable by anyone with the public anon key and the
// session UUID (handed out by the unauthenticated by-code join), so broadcasts
// on it cannot be trusted by themselves. The web server signs every command
// with the session's bearer secret (which we hold — we created the session);
// we recompute the HMAC and drop anything unsigned or mismatched. Mirrors
// sundaystage-web/lib/commandSig.ts — keep the payload format in sync.

export async function verifyRemoteCommand(
  secret: string,
  sessionId: string,
  cmd: string,
  cmdSeq: number,
  sig: string | undefined,
): Promise<boolean> {
  if (!sig) return false;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(`${sessionId}:${cmd}:${cmdSeq}`),
  );
  return base64url(new Uint8Array(mac)) === sig;
}

function base64url(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

export interface WebShareSession {
  id: string;
  code: string;
  secret: string;
}

export type ShareStatus = "off" | "starting" | "sharing" | "error";

/** A remote command from the web operator (the desktop maps these to actions). */
export type RemoteCommand = "next" | "prev" | "black" | "logo" | "clear";

/** Realtime topic the web operator broadcasts remote-control commands on. */
export function commandsTopic(sessionId: string): string {
  return `stage:session:${sessionId}:commands`;
}

/**
 * The commands channel is PRIVATE. Supabase Realtime authorizes every
 * subscriber against the stage-web `realtime.messages` RLS policy, so a forged
 * anon `.send()` of a command is denied — closing the hole where anyone who
 * learned the session UUID could hijack the desktop's slide control. The web
 * `/command` route dual-sends (public + private) during the fleet-upgrade
 * window, so this build listens ONLY on the protected private topic.
 * (Coordinated with sundaystage-web branch feat/rt-hardening-coordinated.)
 */
export const COMMANDS_CHANNEL_CONFIG = {
  config: { private: true },
} as const;

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
  /** The next slide's content, for the web scene monitor (never sensitive). */
  next?: NextSlideInfo | null;
  active: boolean;
  baseUrl?: string;
  onCommand?: (cmd: RemoteCommand) => void;
}): WebShareController {
  const { frame, appearance, next, active, onCommand } = opts;
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
    pending.current = liveFrameToWebFrame(frame, appearance, next);
    void drain(session);
  }, [frame, appearance, next, active, session, drain]);

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
        // Private channel (receive authorized by the wildcard SELECT policy on
        // stage:session:%), matching the web displays' subscription mode.
        const channel = supabase.channel(
          commandsTopic(session.id),
          COMMANDS_CHANNEL_CONFIG,
        );
        channel.on("broadcast", { event: "command" }, (msg) => {
          const payload = (msg.payload ?? {}) as {
            cmd?: RemoteCommand;
            cmd_seq?: number;
            sig?: string;
          };
          const seq = payload.cmd_seq ?? 0;
          if (seq <= lastCmdSeq || !payload.cmd) return; // replay / stale guard
          const { cmd, sig } = payload;
          void (async () => {
            // Only the web server can sign with the session secret — an
            // unsigned/forged broadcast must never drive the live output.
            if (
              !(await verifyRemoteCommand(
                session.secret,
                session.id,
                cmd,
                seq,
                sig,
              ))
            )
              return;
            if (seq <= lastCmdSeq) return; // re-check: another command may have won the await
            lastCmdSeq = seq;
            onCommandRef.current?.(cmd);
          })();
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
