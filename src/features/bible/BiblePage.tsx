/**
 * Bible browser — Phase 7.1.
 *
 * Pick a translation, browse book → chapter → verses, look up a reference
 * ("John 3:16" / "Sal 23"), full-text search ("shepherd"), compare two
 * translations side by side, and add a passage to a service (which the cue
 * compiler turns into slides).
 */
import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { BookOpen, Plus, Search } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { BibleVerse, Library } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { Button, Select } from "@/components/ui";

interface Props {
  library: Library;
}

export function BiblePage({ library }: Props) {
  const [primaryId, setPrimaryId] = useState<string | null>(null);
  const [compareId, setCompareId] = useState<string | null>(null);
  const [book, setBook] = useState<string | null>(null);
  const [chapter, setChapter] = useState<number | null>(null);
  const [range, setRange] = useState<{ start: number; end: number } | null>(
    null,
  );
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<BibleVerse[] | null>(null);
  const [addMsg, setAddMsg] = useState<string | null>(null);

  const translations = useQuery({
    queryKey: ["bibleTranslations"],
    queryFn: () => ipc.bible.translations(),
  });

  // Default to the first translation once loaded.
  useEffect(() => {
    if (!primaryId && translations.data && translations.data.length > 0) {
      setPrimaryId(translations.data[0].id);
    }
  }, [translations.data, primaryId]);

  const books = useQuery({
    queryKey: ["bibleBooks", primaryId],
    queryFn: () => ipc.bible.books(primaryId!),
    enabled: !!primaryId,
  });
  const chapters = useQuery({
    queryKey: ["bibleChapters", primaryId, book],
    queryFn: () => ipc.bible.chapters(primaryId!, book!),
    enabled: !!primaryId && !!book,
  });
  const passage = useQuery({
    queryKey: ["biblePassage", primaryId, book, chapter],
    queryFn: () => ipc.bible.passage(primaryId!, book!, chapter!, null, null),
    enabled: !!primaryId && !!book && chapter != null,
  });
  const comparePassage = useQuery({
    queryKey: ["biblePassageCompare", compareId, book, chapter],
    queryFn: () => ipc.bible.passage(compareId!, book!, chapter!, null, null),
    enabled: !!compareId && !!book && chapter != null,
  });

  const compareByVerse = useMemo(() => {
    const map = new Map<number, string>();
    for (const v of comparePassage.data ?? []) map.set(Number(v.verse), v.text);
    return map;
  }, [comparePassage.data]);

  function selectChapter(b: string, c: number) {
    setBook(b);
    setChapter(c);
    setRange(null);
    setHits(null);
  }

  async function runSearch() {
    if (!primaryId || query.trim() === "") return;
    setAddMsg(null);
    // A reference ("John 3:16") jumps to the passage; otherwise full-text.
    try {
      const p = await ipc.bible.lookup(primaryId, query);
      if (p.verses.length > 0) {
        const first = p.verses[0];
        setBook(first.book);
        setChapter(Number(first.chapter));
        setRange({
          start: Number(p.verses[0].verse),
          end: Number(p.verses[p.verses.length - 1].verse),
        });
        setHits(null);
        return;
      }
    } catch {
      /* not a reference — fall through to full-text */
    }
    setHits(await ipc.bible.search(query, primaryId));
  }

  async function addToService() {
    if (!primaryId || !book || chapter == null) return;
    setAddMsg(null);
    const upcoming = await ipc.service.upcoming(library.id, 0, 1);
    const svc =
      upcoming[0] ??
      (await ipc.service.create(library.id, "Ny tjeneste", Date.now()));
    await ipc.bible.addToService(
      svc.id,
      primaryId,
      book,
      chapter,
      range?.start ?? null,
      range?.end ?? null,
    );
    setAddMsg(`Lagt til i «${svc.name}».`);
  }

  const inRange = (v: number) =>
    range ? v >= range.start && v <= range.end : true;

  return (
    <div className="flex h-full flex-col bg-[var(--color-bg)]">
      {/* Header */}
      <header className="flex flex-wrap items-center gap-3 border-b border-[var(--color-border)] px-5 py-3">
        <BookOpen size={18} className="text-[var(--color-accent)]" />
        <h1 className="text-lg font-semibold">Bibel</h1>
        <div className="flex-1" />
        <label className="text-xs text-[var(--color-fg-muted)]">
          Oversettelse
        </label>
        <Select
          className="w-44"
          value={primaryId ?? ""}
          onChange={(e) => setPrimaryId(e.target.value)}
        >
          {(translations.data ?? []).map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </Select>
        <label className="text-xs text-[var(--color-fg-muted)]">
          Sammenlign
        </label>
        <Select
          className="w-44"
          value={compareId ?? ""}
          onChange={(e) => setCompareId(e.target.value || null)}
        >
          <option value="">Ingen</option>
          {(translations.data ?? [])
            .filter((t) => t.id !== primaryId)
            .map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
        </Select>
      </header>

      {/* Search bar */}
      <div className="flex items-center gap-2 border-b border-[var(--color-border)] px-5 py-2">
        <div className="relative flex-1">
          <Search
            size={14}
            className="absolute top-1/2 left-2.5 -translate-y-1/2 text-[var(--color-fg-muted)]"
          />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && runSearch()}
            placeholder="Slå opp en referanse (John 3:16) eller søk i teksten (shepherd)…"
            className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] py-1.5 pr-3 pl-8 text-sm focus:border-[var(--color-accent)] focus:outline-none"
          />
        </div>
        <Button size="sm" onClick={runSearch}>
          Søk
        </Button>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-[200px_1fr]">
        {/* Books + chapters */}
        <aside className="overflow-y-auto border-r border-[var(--color-border)] p-2">
          {(books.data ?? []).map((b) => (
            <div key={b.book} className="mb-0.5">
              <button
                type="button"
                onClick={() => setBook(b.book === book ? null : b.book)}
                className={cn(
                  "w-full rounded-md px-2.5 py-1.5 text-left text-sm",
                  b.book === book
                    ? "bg-[var(--color-bg-surface)] text-[var(--color-fg)]"
                    : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]/60",
                )}
              >
                {b.display}
              </button>
              {b.book === book && (
                <div className="flex flex-wrap gap-1 px-2 py-1.5">
                  {(chapters.data ?? []).map((c) => (
                    <button
                      key={c}
                      type="button"
                      onClick={() => selectChapter(b.book, c)}
                      className={cn(
                        "h-7 w-7 rounded text-xs tabular-nums",
                        c === chapter
                          ? "bg-[var(--color-accent)] font-bold text-[var(--color-accent-fg)]"
                          : "bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]",
                      )}
                    >
                      {c}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))}
        </aside>

        {/* Reading / results pane */}
        <main className="overflow-y-auto p-6">
          {hits ? (
            <SearchResults
              hits={hits}
              onPick={(v) => selectChapter(v.book, Number(v.chapter))}
            />
          ) : chapter != null && book ? (
            <div>
              <div className="mb-4 flex items-center gap-3">
                <h2 className="text-xl font-semibold">
                  {(books.data ?? []).find((b) => b.book === book)?.display ??
                    book}{" "}
                  {chapter}
                </h2>
                <Button size="sm" variant="outline" onClick={addToService}>
                  <Plus size={14} /> Legg til i tjeneste
                </Button>
                {addMsg && (
                  <span className="text-xs text-[var(--color-success)]">
                    {addMsg}
                  </span>
                )}
              </div>
              <div className="max-w-3xl space-y-2">
                {(passage.data ?? []).map((v) => (
                  <div
                    key={v.id}
                    className={cn(
                      "grid grid-cols-[2rem_1fr] gap-2 rounded px-1 py-0.5",
                      !inRange(Number(v.verse)) && "opacity-40",
                      compareId && "md:grid-cols-[2rem_1fr_1fr]",
                    )}
                  >
                    <span className="pt-0.5 text-right font-mono text-[11px] text-[var(--color-accent)]">
                      {v.verse}
                    </span>
                    <p className="text-sm leading-relaxed">{v.text}</p>
                    {compareId && (
                      <p className="border-l border-[var(--color-border)] pl-2 text-sm leading-relaxed text-[var(--color-fg-muted)]">
                        {compareByVerse.get(Number(v.verse)) ?? "—"}
                      </p>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="grid h-full place-items-center text-center text-sm text-[var(--color-fg-muted)]">
              <div>
                <p>Velg en bok og et kapittel, eller søk over.</p>
                <p className="mt-1 text-xs">
                  Innebygd: King James Version + Bibelen 1930 (utvalgte
                  passasjer). Full nedlasting kommer.
                </p>
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

function SearchResults({
  hits,
  onPick,
}: {
  hits: BibleVerse[];
  onPick: (v: BibleVerse) => void;
}) {
  if (hits.length === 0) {
    return <p className="text-sm text-[var(--color-fg-muted)]">Ingen treff.</p>;
  }
  return (
    <div className="max-w-3xl space-y-1.5">
      <p className="mb-2 text-xs text-[var(--color-fg-muted)]">
        {hits.length} treff
      </p>
      {hits.map((v) => (
        <button
          key={v.id}
          type="button"
          onClick={() => onPick(v)}
          className="block w-full rounded-md border border-[var(--color-border)] px-3 py-2 text-left hover:bg-[var(--color-bg-surface)]"
        >
          <div className="text-[11px] font-medium text-[var(--color-accent)]">
            {v.book} {v.chapter}:{v.verse}
          </div>
          <p className="text-sm">{v.text}</p>
        </button>
      ))}
    </div>
  );
}
