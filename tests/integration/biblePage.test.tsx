// Integration — the Bible browser's ⌘K deep-open behaviour (Phase 2.3 polish).
//
// A bible search hit selected in the command palette resolves to a
// `BibleDeepLink` (covered in workspace.test.ts); here we assert the
// *consumer* side: when BiblePage receives that deep-link it opens the exact
// passage, highlights the matched verse (full opacity vs. the dimmed rest),
// updates the reading header to the verse reference, and scrolls the verse
// into view. The Tauri IPC layer is mocked so the test runs without a backend.
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { BiblePage } from "@/features/bible/BiblePage";
import { formatVerseSuffix } from "@/features/bible/bibleFormat";
import type { BibleVerse, Library } from "@/lib/bindings";

// ── IPC mock ─────────────────────────────────────────────────────────────────

const verse = (n: number, text: string): BibleVerse => ({
  id: `john-3-${n}`,
  translation_id: "kjv",
  book: "John",
  book_order: 43n,
  chapter: 3n,
  verse: BigInt(n),
  text,
  created_at: 0n,
});

const PASSAGE: BibleVerse[] = [
  verse(14, "And as Moses lifted up the serpent in the wilderness…"),
  verse(15, "That whosoever believeth in him should not perish…"),
  verse(16, "For God so loved the world, that he gave his only begotten Son…"),
  verse(17, "For God sent not his Son into the world to condemn the world…"),
];

vi.mock("@/lib/ipc", () => ({
  ipc: {
    bible: {
      translations: vi.fn(async () => [
        {
          id: "kjv",
          code: "kjv",
          name: "King James Version",
          language: "en",
          public_domain: 1n,
          created_at: 0n,
        },
      ]),
      books: vi.fn(async () => [
        { book: "John", book_order: 43n, display: "John" },
      ]),
      chapters: vi.fn(async () => [1, 2, 3]),
      passage: vi.fn(async () => PASSAGE),
    },
  },
}));

const LIBRARY = { id: "lib-1", name: "Test" } as unknown as Library;

function renderBible(deepLink: Parameters<typeof BiblePage>[0]["deepLink"]) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <BiblePage library={LIBRARY} deepLink={deepLink} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  // jsdom has no layout engine; the deep-link effect calls scrollIntoView.
  Element.prototype.scrollIntoView = vi.fn();
});

// Unmount each render so multiple deep-open cases don't leave stale BiblePage
// instances in the DOM (testing-library has no implicit auto-cleanup here).
afterEach(cleanup);

// ── formatVerseSuffix (pure) ──────────────────────────────────────────────────

describe("formatVerseSuffix", () => {
  it("is empty for a whole chapter (no range)", () => {
    expect(formatVerseSuffix(null)).toBe("");
  });
  it("shows a single verse", () => {
    expect(formatVerseSuffix({ start: 16, end: 16 })).toBe(":16");
  });
  it("shows a verse range with an en-dash", () => {
    expect(formatVerseSuffix({ start: 4, end: 7 })).toBe(":4–7");
  });
});

// ── ⌘K deep-open → BiblePage ─────────────────────────────────────────────────

describe("BiblePage deep-open", () => {
  it("opens the passage, highlights the verse, and labels the reference", async () => {
    renderBible({ book: "John", chapter: 3, verseStart: 16, verseEnd: null });

    // Reading header reflects the exact reference, not just the chapter.
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: /John 3:16/ })).toBeVisible(),
    );

    // The matched verse renders at full opacity *and* carries the explicit
    // accent highlight; out-of-range verses are dimmed and never highlighted.
    const matched = await screen.findByText(/For God so loved the world/);
    const matchedRow = matched.closest("[data-verse]") as HTMLElement;
    expect(matchedRow).toHaveAttribute("data-verse", "16");
    expect(matchedRow).toHaveAttribute("data-matched", "true");
    expect(matchedRow.className).not.toContain("opacity-40");
    expect(matchedRow.className).toContain("ring-[var(--color-accent)]");

    const dimmed = screen
      .getByText(/And as Moses lifted up/)
      .closest("[data-verse]") as HTMLElement;
    expect(dimmed.className).toContain("opacity-40");
    expect(dimmed).not.toHaveAttribute("data-matched");
    expect(dimmed.className).not.toContain("ring-[var(--color-accent)]");
  });

  it("highlights every verse in a multi-verse range", async () => {
    renderBible({ book: "John", chapter: 3, verseStart: 15, verseEnd: 16 });

    await screen.findByText(/For God so loved the world/);
    const v15 = screen
      .getByText(/That whosoever believeth/)
      .closest("[data-verse]") as HTMLElement;
    const v16 = screen
      .getByText(/For God so loved the world/)
      .closest("[data-verse]") as HTMLElement;
    expect(v15).toHaveAttribute("data-matched", "true");
    expect(v16).toHaveAttribute("data-matched", "true");

    // A verse outside the range is dimmed and unhighlighted.
    const v17 = screen
      .getByText(/condemn the world/)
      .closest("[data-verse]") as HTMLElement;
    expect(v17).not.toHaveAttribute("data-matched");
    expect(v17.className).toContain("opacity-40");
  });

  it("scrolls the matched verse into view (centered)", async () => {
    renderBible({ book: "John", chapter: 3, verseStart: 16, verseEnd: null });

    await screen.findByText(/For God so loved the world/);
    await waitFor(() =>
      expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith(
        expect.objectContaining({ block: "center" }),
      ),
    );
  });
});
