import { describe, expect, it } from "vitest";
import {
  COMMANDS_CHANNEL_CONFIG,
  commandsTopic,
  liveFrameToWebFrame,
  verifyRemoteCommand,
} from "@/lib/webShare";
import webframeSchema from "@/lib/webframe.schema.json";
import type { LiveFrame, OutputAppearance, SlideContent } from "@/lib/bindings";

const appearance: OutputAppearance = {
  text_scale: 1.25,
  text_color: "#ffffff",
  bg_color: "#0a1730",
  h_align: "center",
  show_section_label: true,
  uppercase: false,
  line_height: 1.1,
};

function slide(over: Partial<SlideContent> = {}): LiveFrame {
  return {
    kind: "slide",
    slide_content: {
      section_label: "Vers 1",
      text_lines: ["Stor er din trofasthet"],
      translation_lines: null,
      reference: null,
      sensitive_slide: false,
      ...over,
    },
  };
}

describe("liveFrameToWebFrame", () => {
  it("maps a lyric slide and threads appearance into the web shape", () => {
    const wf = liveFrameToWebFrame(slide(), appearance);
    expect(wf).toMatchObject({
      v: 1,
      kind: "slide",
      text_lines: ["Stor er din trofasthet"],
      section_label: "Vers 1",
    });
    expect(wf.appearance).toEqual({
      bg_color: "#0a1730",
      text_color: "#ffffff",
      font_scale: 1.25,
    });
  });

  it("gates a sensitive slide to the neutral placeholder (private text never leaves)", () => {
    const wf = liveFrameToWebFrame(
      slide({ text_lines: ["Forbønn: Navn Navnesen"], sensitive_slide: true }),
      appearance,
    );
    expect(wf.kind).toBe("message");
    expect(wf.message).toBe("Tjeneste pågår");
    expect(JSON.stringify(wf)).not.toContain("Navnesen");
  });

  it("maps black/logo/message", () => {
    expect(liveFrameToWebFrame({ kind: "black" }).kind).toBe("black");
    expect(liveFrameToWebFrame({ kind: "logo" }).kind).toBe("logo");
    expect(
      liveFrameToWebFrame({ kind: "message", text: "Velkommen" }),
    ).toMatchObject({
      kind: "message",
      message: "Velkommen",
    });
  });

  it("carries scripture reference + translation lines", () => {
    const wf = liveFrameToWebFrame(
      slide({ reference: "Joh 3,16", translation_lines: ["For God so loved"] }),
    );
    expect(wf.reference).toBe("Joh 3,16");
    expect(wf.translation_lines).toEqual(["For God so loved"]);
  });

  it("threads the next slide into next_lines/next_label (scene monitor)", () => {
    const wf = liveFrameToWebFrame(slide(), appearance, {
      lines: ["Neste linje"],
      label: "Refreng",
    });
    expect(wf.next_lines).toEqual(["Neste linje"]);
    expect(wf.next_label).toBe("Refreng");
  });

  it("drops next on a sensitive current slide (nothing sensitive leaks)", () => {
    const wf = liveFrameToWebFrame(
      slide({ sensitive_slide: true }),
      appearance,
      { lines: ["Neste"], label: null },
    );
    expect(wf.kind).toBe("message");
    expect(wf.next_lines).toBeUndefined();
  });

  it("clamps font_scale into the server's accepted range instead of getting the frame rejected", () => {
    const big = liveFrameToWebFrame(slide(), { ...appearance, text_scale: 9 });
    expect(big.appearance?.font_scale).toBe(3);
    const small = liveFrameToWebFrame(slide(), {
      ...appearance,
      text_scale: 0.05,
    });
    expect(small.appearance?.font_scale).toBe(0.3);
    const bad = liveFrameToWebFrame(slide(), {
      ...appearance,
      text_scale: Number.NaN,
    });
    expect(bad.appearance?.font_scale).toBe(1);
  });

  it("truncates oversized text fields instead of getting the frame rejected", () => {
    const wf = liveFrameToWebFrame(
      slide({
        text_lines: Array.from({ length: 60 }, (_, i) => "x".repeat(600 + i)),
        section_label: "s".repeat(200),
        reference: "r".repeat(200),
      }),
      { ...appearance, bg_color: "#".repeat(64) },
    );
    expect(wf.text_lines).toHaveLength(40);
    expect(wf.text_lines?.[0]).toHaveLength(500);
    expect(wf.section_label).toHaveLength(80);
    expect(wf.reference).toHaveLength(120);
    expect(wf.appearance?.bg_color).toHaveLength(32);
  });
});

// ── Contract: every frame we send must validate against the web's schema ─────
// src/lib/webframe.schema.json is vendored from sundaystage-web (generated
// from its zod WebFrame; a web-side test pins the copy there). The server
// rejects a frame WHOLESALE on any violation — freezing displays — so this is
// the CI tripwire against contract drift on our side.

interface JsonSchema {
  type?: string;
  const?: unknown;
  enum?: unknown[];
  maxLength?: number;
  maxItems?: number;
  minimum?: number;
  maximum?: number;
  items?: JsonSchema;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  additionalProperties?: boolean | JsonSchema;
  anyOf?: JsonSchema[];
}

function validate(value: unknown, schema: JsonSchema, path = "$"): string[] {
  if (schema.anyOf) {
    const branches = schema.anyOf.map((s) => validate(value, s, path));
    return branches.some((b) => b.length === 0) ? [] : branches.flat();
  }
  const errs: string[] = [];
  switch (schema.type) {
    case "object": {
      if (typeof value !== "object" || value === null || Array.isArray(value))
        return [`${path}: expected object`];
      const obj = value as Record<string, unknown>;
      for (const key of schema.required ?? [])
        if (obj[key] === undefined) errs.push(`${path}.${key}: missing`);
      for (const [key, v] of Object.entries(obj)) {
        if (v === undefined) continue;
        const prop = schema.properties?.[key];
        if (!prop) {
          if (schema.additionalProperties === false)
            errs.push(`${path}.${key}: unexpected property`);
          continue;
        }
        errs.push(...validate(v, prop, `${path}.${key}`));
      }
      return errs;
    }
    case "array": {
      if (!Array.isArray(value)) return [`${path}: expected array`];
      if (schema.maxItems != null && value.length > schema.maxItems)
        errs.push(`${path}: more than ${schema.maxItems} items`);
      if (schema.items)
        value.forEach((v, i) =>
          errs.push(
            ...validate(v, schema.items as JsonSchema, `${path}[${i}]`),
          ),
        );
      return errs;
    }
    case "string": {
      if (typeof value !== "string") return [`${path}: expected string`];
      if (schema.maxLength != null && value.length > schema.maxLength)
        errs.push(`${path}: longer than ${schema.maxLength}`);
      if (schema.enum && !schema.enum.includes(value))
        errs.push(`${path}: not in enum`);
      return errs;
    }
    case "number": {
      if (typeof value !== "number" || !Number.isFinite(value))
        return [`${path}: expected number`];
      if (schema.const !== undefined && value !== schema.const)
        errs.push(`${path}: expected const ${String(schema.const)}`);
      if (schema.minimum != null && value < schema.minimum)
        errs.push(`${path}: below ${schema.minimum}`);
      if (schema.maximum != null && value > schema.maximum)
        errs.push(`${path}: above ${schema.maximum}`);
      return errs;
    }
    case "null":
      return value === null ? [] : [`${path}: expected null`];
    default:
      return errs;
  }
}

function assertValidWebFrame(frame: unknown) {
  const schema = (webframeSchema as { definitions: Record<string, JsonSchema> })
    .definitions.WebFrame;
  expect(validate(frame, schema)).toEqual([]);
}

describe("WebFrame contract (vendored schema from sundaystage-web)", () => {
  it("plain and decorated slides validate", () => {
    assertValidWebFrame(liveFrameToWebFrame(slide()));
    assertValidWebFrame(
      liveFrameToWebFrame(
        slide({
          reference: "Joh 3,16",
          translation_lines: ["For God so loved"],
        }),
        appearance,
        { lines: ["neste"], label: "Refreng" },
      ),
    );
  });

  it("black / logo / message / sensitive frames validate", () => {
    assertValidWebFrame(liveFrameToWebFrame({ kind: "black" }, appearance));
    assertValidWebFrame(liveFrameToWebFrame({ kind: "logo" }, appearance));
    assertValidWebFrame(
      liveFrameToWebFrame({ kind: "message", text: "Velkommen" }, appearance),
    );
    assertValidWebFrame(
      liveFrameToWebFrame(slide({ sensitive_slide: true }), appearance),
    );
  });

  it("extreme inputs still validate after clamping (never a wholesale 400)", () => {
    assertValidWebFrame(
      liveFrameToWebFrame(
        slide({
          text_lines: Array.from({ length: 80 }, () => "y".repeat(2000)),
          section_label: "s".repeat(500),
          reference: "r".repeat(500),
          translation_lines: Array.from({ length: 80 }, () => "t".repeat(2000)),
        }),
        {
          ...appearance,
          text_scale: 99,
          bg_color: "b".repeat(100),
          text_color: "c".repeat(100),
        },
        {
          lines: Array.from({ length: 80 }, () => "n".repeat(2000)),
          label: "l".repeat(500),
        },
      ),
    );
    assertValidWebFrame(
      liveFrameToWebFrame(
        { kind: "message", text: "m".repeat(5000) },
        { ...appearance, text_scale: -5 },
      ),
    );
  });

  it("the validator itself rejects out-of-contract frames (sanity)", () => {
    const schema = (
      webframeSchema as { definitions: Record<string, JsonSchema> }
    ).definitions.WebFrame;
    expect(
      validate({ v: 1, kind: "slide", appearance: { font_scale: 9 } }, schema),
    ).not.toEqual([]);
    expect(validate({ v: 2, kind: "slide" }, schema)).not.toEqual([]);
  });
});

// ── Remote-command signature verification ─────────────────────────────────────

async function sign(
  secret: string,
  sessionId: string,
  cmd: string,
  cmdSeq: number,
): Promise<string> {
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
  let bin = "";
  for (const b of new Uint8Array(mac)) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

describe("verifyRemoteCommand", () => {
  const SECRET = "a".repeat(64);
  const ID = "0197f9a0-0000-7000-8000-000000000001";

  it("accepts a command signed with the session secret", async () => {
    const sig = await sign(SECRET, ID, "next", 3);
    expect(await verifyRemoteCommand(SECRET, ID, "next", 3, sig)).toBe(true);
  });

  it("rejects unsigned commands (forged broadcasts are inert)", async () => {
    expect(await verifyRemoteCommand(SECRET, ID, "next", 3, undefined)).toBe(
      false,
    );
    expect(await verifyRemoteCommand(SECRET, ID, "next", 3, "")).toBe(false);
  });

  it("rejects a signature under the wrong secret or tampered fields", async () => {
    const sig = await sign("b".repeat(64), ID, "next", 3);
    expect(await verifyRemoteCommand(SECRET, ID, "next", 3, sig)).toBe(false);
    const good = await sign(SECRET, ID, "next", 3);
    expect(await verifyRemoteCommand(SECRET, ID, "black", 3, good)).toBe(false);
    expect(await verifyRemoteCommand(SECRET, ID, "next", 4, good)).toBe(false);
  });
});

describe("remote-control commands channel", () => {
  it("derives the per-session commands topic", () => {
    expect(commandsTopic("abc-123")).toBe("stage:session:abc-123:commands");
  });

  it("subscribes as a PRIVATE channel (rejects forged anon commands)", () => {
    // A public channel would let anyone who learned the session UUID `.send()`
    // a forged command and hijack the desktop's slide control. Private makes
    // Realtime authorize the subscriber against the stage-web RLS policy.
    expect(COMMANDS_CHANNEL_CONFIG.config.private).toBe(true);
  });
});
