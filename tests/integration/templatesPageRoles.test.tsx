// Integration — per-template stage-display role assignment UI (Phase 8).
//
// Proves the wiring of the role dropdown + live inspector preview in
// TemplatesPage: selecting a template shows its role preview, changing the
// role updates the preview panels live (driven by the pure `panelsForRole`
// mapping), and the assignment persists in localStorage so it survives a
// remount (page reload). The Tauri IPC layer is mocked so it runs with no
// backend, using the offline demo-style template fixtures.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  render,
  screen,
  within,
  cleanup,
  fireEvent,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { TemplatesPage } from "@/features/services/TemplatesPage";
import { useLocale } from "@/lib/i18n";
import type { CueSpec, ServiceTemplate } from "@/lib/bindings";

// ── Fixtures ───────────────────────────────────────────────────────────────────

const SPECS: CueSpec[] = [
  { kind: "song", label: "Lovsang 1", notes: null },
  { kind: "bible", label: "Tekst", notes: null },
];

function tmpl(id: string, name: string, builtin: boolean): ServiceTemplate {
  return {
    id,
    name,
    description: null,
    cue_specs: JSON.stringify(SPECS),
    is_builtin: BigInt(builtin ? 1 : 0),
    created_at: BigInt(0),
    updated_at: BigInt(0),
  };
}

const TEMPLATES: ServiceTemplate[] = [
  tmpl("builtin-1", "Standardgudstjeneste", true),
  tmpl("custom-1", "Min mal", false),
];

// ── IPC mock ─────────────────────────────────────────────────────────────────

vi.mock("@/lib/ipc", () => ({
  ipc: {
    serviceTemplate: {
      list: vi.fn(async () => TEMPLATES),
      create: vi.fn(),
      delete: vi.fn(),
      apply: vi.fn(),
    },
    service: { upcoming: vi.fn(async () => []) },
    parseCueSpecs: (template: ServiceTemplate) =>
      JSON.parse(template.cue_specs) as CueSpec[],
  },
}));

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <TemplatesPage libraryId="lib-1" />
    </QueryClientProvider>,
  );
}

/** The card whose <h3> title matches `name`. */
function cardFor(name: string): HTMLElement {
  const heading = screen.getAllByText(name).find((el) => el.tagName === "H3");
  expect(heading, `card titled ${name}`).toBeTruthy();
  // climb to the clickable card container (the role <select> lives inside).
  let el: HTMLElement | null = heading as HTMLElement;
  while (el && !el.querySelector("select")) el = el.parentElement;
  expect(el).toBeTruthy();
  return el as HTMLElement;
}

beforeEach(() => {
  localStorage.clear();
  // Pin the locale so label-text queries are stable (default is Norwegian).
  useLocale.getState().setLang("en");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("TemplatesPage role assignment", () => {
  it("renders a role dropdown defaulting to worship leader on each card", async () => {
    renderPage();
    const card = await screen.findByText("Min mal");
    expect(card).toBeInTheDocument();

    const selects = screen.getAllByLabelText("Stage role");
    expect(selects.length).toBe(TEMPLATES.length);
    for (const sel of selects) {
      expect((sel as HTMLSelectElement).value).toBe("worship-leader");
    }
  });

  it("shows the live preview when a template is selected and updates as role changes", async () => {
    renderPage();
    await screen.findByText("Min mal");

    // Select the custom template card.
    const card = cardFor("Min mal");
    fireEvent.click(card);

    // Worship leader → every panel is on.
    const preview = screen.getByTestId("role-preview-panels");
    const clockRow = () =>
      preview.querySelector('[data-panel="showClock"]') as HTMLElement;
    expect(clockRow().getAttribute("data-on")).toBe("1");

    // Change role to musician → clock turns off, section label stays on.
    const select = within(card).getByLabelText(
      "Stage role",
    ) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "musician" } });

    expect(clockRow().getAttribute("data-on")).toBe("0");
    expect(
      (
        preview.querySelector('[data-panel="showSectionLabel"]') as HTMLElement
      ).getAttribute("data-on"),
    ).toBe("1");

    // Congregation → only the current slide remains.
    fireEvent.change(select, { target: { value: "congregation" } });
    expect(
      (
        preview.querySelector('[data-panel="showCurrentSlide"]') as HTMLElement
      ).getAttribute("data-on"),
    ).toBe("1");
    expect(
      (
        preview.querySelector('[data-panel="showNextSlide"]') as HTMLElement
      ).getAttribute("data-on"),
    ).toBe("0");
  });

  it("persists the role across a remount (page reload)", async () => {
    const first = renderPage();
    await screen.findByText("Min mal");

    const select = within(cardFor("Min mal")).getByLabelText(
      "Stage role",
    ) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "operator" } });
    expect(localStorage.getItem("ss-template-roles")).toContain("operator");

    // Unmount and re-render — simulates a page reload reading from localStorage.
    first.unmount();
    renderPage();
    await screen.findByText("Min mal");

    const reloaded = within(cardFor("Min mal")).getByLabelText(
      "Stage role",
    ) as HTMLSelectElement;
    expect(reloaded.value).toBe("operator");
  });
});
