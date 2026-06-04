/**
 * Pure gallery selection/apply tests (deep-stage-2). No IPC/DOM.
 */
import { describe, it, expect } from "vitest";

import type { Template, TemplateLayout } from "@/lib/bindings";
import { emptyPayload } from "./slotMapping";
import {
  gallerySections,
  buildApplyPlan,
  filterTemplates,
} from "./templateGallery";

function tmpl(
  id: string,
  name: string,
  isBuiltin: boolean,
  layout: TemplateLayout = { slots: [] },
): Template {
  return {
    id,
    library_id: isBuiltin ? null : "lib-1",
    name,
    slots: JSON.stringify(layout),
    is_builtin: isBuiltin ? 1n : 0n,
    created_at: 0n,
    updated_at: 0n,
  };
}

describe("gallerySections", () => {
  it("partitions built-ins first, then custom", () => {
    const list = [
      tmpl("b1", "Lyrics", true),
      tmpl("c1", "Our Look", false),
      tmpl("b2", "Title", true),
    ];
    const sections = gallerySections(list);
    expect(sections.map((s) => s.kind)).toEqual(["builtin", "custom"]);
    expect(sections[0].templates.map((t) => t.id)).toEqual(["b1", "b2"]);
    expect(sections[1].templates.map((t) => t.id)).toEqual(["c1"]);
  });

  it("drops an empty section", () => {
    const sections = gallerySections([tmpl("b1", "Lyrics", true)]);
    expect(sections).toHaveLength(1);
    expect(sections[0].kind).toBe("builtin");
  });

  it("handles an empty list", () => {
    expect(gallerySections([])).toEqual([]);
  });
});

describe("buildApplyPlan", () => {
  const announcement = tmpl("ann", "Announcement", true, {
    slots: [
      {
        name: "title",
        role: "title",
        rect: { x: 0, y: 0, w: 1, h: 1 },
        align: "center",
        valign: "middle",
        size_scale: 1,
      },
      {
        name: "body",
        role: "body",
        rect: { x: 0, y: 0, w: 1, h: 1 },
        align: "center",
        valign: "middle",
        size_scale: 1,
      },
    ],
  });

  it("produces the template id and the mapped slot text", () => {
    const plan = buildApplyPlan(announcement, {
      ...emptyPayload(),
      title: "Notice",
      body: "Meeting tonight",
    });
    expect(plan.templateId).toBe("ann");
    expect(plan.slotText).toEqual({ title: "Notice", body: "Meeting tonight" });
  });

  it("omits unfilled slots from the plan", () => {
    const plan = buildApplyPlan(announcement, {
      ...emptyPayload(),
      title: "Only title",
    });
    expect(plan.slotText).toEqual({ title: "Only title" });
  });

  it("tolerates a template with malformed slots JSON (empty plan)", () => {
    const broken: Template = { ...announcement, slots: "not json" };
    const plan = buildApplyPlan(broken, {
      ...emptyPayload(),
      title: "x",
    });
    expect(plan.slotText).toEqual({});
  });
});

describe("filterTemplates", () => {
  const list = [
    tmpl("a", "Lyrics Centered", true),
    tmpl("b", "Bible Verse", true),
    tmpl("c", "Announcement", true),
  ];

  it("returns all on an empty query", () => {
    expect(filterTemplates(list, "  ")).toHaveLength(3);
  });

  it("matches case-insensitively on name", () => {
    expect(filterTemplates(list, "bible").map((t) => t.id)).toEqual(["b"]);
    expect(filterTemplates(list, "LYRICS").map((t) => t.id)).toEqual(["a"]);
  });
});
