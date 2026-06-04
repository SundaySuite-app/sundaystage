/**
 * Pure slot-mapping tests (deep-stage-2).
 *
 * Mirrors the Rust `services::theme::map_content_to_slots` test suite so the
 * preview (TS) and the persisted render (Rust) cannot drift. No IPC/DOM.
 */
import { describe, it, expect } from "vitest";

import type {
  SlideContentPayload,
  TemplateLayout,
  TemplateSlot,
  SlotRole,
  HAlign,
  VAlign,
} from "@/lib/bindings";
import {
  mapContentToSlots,
  derivePayloadFromDoc,
  emptyPayload,
} from "./slotMapping";

function slot(name: string, role: SlotRole): TemplateSlot {
  return {
    name,
    role,
    rect: { x: 0, y: 0, w: 1, h: 1 },
    align: "center" as HAlign,
    valign: "middle" as VAlign,
    size_scale: 1,
  };
}

function layout(...slots: TemplateSlot[]): TemplateLayout {
  return { slots };
}

const announcement = layout(
  slot("title", "title"),
  slot("body", "body"),
  slot("footer", "footer"),
);
const twoColumn = layout(slot("left", "lyrics"), slot("right", "lyrics"));
const titleOnly = layout(slot("title", "title"));
const imageCaption = layout(slot("image", "image"), slot("footer", "footer"));

function payload(p: Partial<SlideContentPayload>): SlideContentPayload {
  return { ...emptyPayload(), ...p };
}

describe("mapContentToSlots", () => {
  it("fills each slot from its role field", () => {
    const map = mapContentToSlots(
      announcement,
      payload({ title: "Youth Camp", body: "Sign up", footer: "July 12" }),
    );
    expect(map).toEqual({
      title: "Youth Camp",
      body: "Sign up",
      footer: "July 12",
    });
  });

  it("leaves a missing-content slot empty, not broken", () => {
    const map = mapContentToSlots(announcement, payload({ title: "Welcome" }));
    expect(map).toEqual({ title: "Welcome" });
    expect("body" in map).toBe(false);
    expect("footer" in map).toBe(false);
  });

  it("treats a blank/whitespace field as empty", () => {
    const map = mapContentToSlots(titleOnly, payload({ title: "  \n  " }));
    expect(map).toEqual({});
  });

  it("ignores payload fields whose role no slot declares", () => {
    const map = mapContentToSlots(
      titleOnly,
      payload({
        title: "Sermon",
        body: "x",
        lyrics: "x",
        reference: "x",
        footer: "x",
        image: "x.png",
      }),
    );
    expect(map).toEqual({ title: "Sermon" });
  });

  it("splits one role across multiple slots by paragraph", () => {
    const map = mapContentToSlots(
      twoColumn,
      payload({ lyrics: "verse one\nline two\n\nverse three\nline four" }),
    );
    expect(map).toEqual({
      left: "verse one\nline two",
      right: "verse three\nline four",
    });
  });

  it("puts unsplittable multi-slot text into the first slot only", () => {
    const map = mapContentToSlots(
      twoColumn,
      payload({ lyrics: "one block\nno blank line" }),
    );
    expect(map).toEqual({ left: "one block\nno blank line" });
  });

  it("is deterministic", () => {
    const p = payload({ title: "A", body: "B", footer: "C" });
    expect(mapContentToSlots(announcement, p)).toEqual(
      mapContentToSlots(announcement, p),
    );
  });

  it("maps an image payload into the image slot verbatim", () => {
    const map = mapContentToSlots(
      imageCaption,
      payload({ image: "asset-123", footer: "Baptism" }),
    );
    expect(map).toEqual({ image: "asset-123", footer: "Baptism" });
  });

  it("yields an empty map for a slotless (blank) template", () => {
    expect(mapContentToSlots(layout(), payload({ title: "x" }))).toEqual({});
  });
});

describe("derivePayloadFromDoc", () => {
  it("collects text into body/lyrics and seeds title from the first line", () => {
    const p = derivePayloadFromDoc({
      background: { type: "color", value: "#000" },
      blocks: [
        {
          type: "text",
          id: "a",
          text: "Hope\nis here",
          rect: { x: 0, y: 0, w: 1, h: 1 },
          align: "center",
          valign: "middle",
          style: {
            family: null,
            size: 64,
            weight: 700,
            color: "#fff",
            italic: false,
            shadow: null,
          },
        },
      ],
    });
    expect(p.title).toBe("Hope");
    expect(p.body).toBe("Hope\nis here");
    expect(p.lyrics).toBe("Hope\nis here");
    expect(p.image).toBeNull();
  });

  it("picks up the first image block as the image field", () => {
    const p = derivePayloadFromDoc({
      background: { type: "color", value: "#000" },
      blocks: [
        {
          type: "image",
          id: "img",
          rect: { x: 0, y: 0, w: 1, h: 1 },
          src: "photo.png",
        },
      ],
    });
    expect(p.image).toBe("photo.png");
    expect(p.body).toBeNull();
    expect(p.title).toBeNull();
  });

  it("returns an all-null payload for an empty doc", () => {
    const p = derivePayloadFromDoc({
      background: { type: "color", value: "#000" },
      blocks: [],
    });
    expect(p).toEqual(emptyPayload());
  });
});
