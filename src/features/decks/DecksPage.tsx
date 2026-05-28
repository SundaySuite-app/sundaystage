/**
 * DecksPage — Phase 3.1.
 *
 * Lists a library's custom decks and opens the slide editor. A "deck" is an
 * ad-hoc slide deck (announcements, sermon points, welcome slides) — the
 * content the slide editor designs.
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LayoutTemplate, Plus, Trash2 } from "lucide-react";

import type { CustomDeck, Library } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { SlideEditor } from "./SlideEditor";

interface Props {
  library: Library;
}

export function DecksPage({ library }: Props) {
  const qc = useQueryClient();
  const [openDeck, setOpenDeck] = useState<CustomDeck | null>(null);

  const decksQuery = useQuery({
    queryKey: ["decks", library.id],
    queryFn: () => ipc.deck.list(library.id),
  });

  const createDeck = useMutation({
    mutationFn: () =>
      ipc.deck.create(
        library.id,
        `Nytt deck ${new Date().toLocaleDateString("no")}`,
      ),
    onSuccess: (deck) => {
      void qc.invalidateQueries({ queryKey: ["decks", library.id] });
      setOpenDeck(deck);
    },
  });

  const deleteDeck = useMutation({
    mutationFn: (id: string) => ipc.deck.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["decks", library.id] }),
  });

  if (openDeck) {
    return <SlideEditor deck={openDeck} onBack={() => setOpenDeck(null)} />;
  }

  const decks = decksQuery.data ?? [];

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-[var(--color-border)] px-6 py-4">
        <h1 className="text-[var(--text-ui-xl)] font-semibold">Decks</h1>
        <span className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-xs text-[var(--color-fg-muted)]">
          {library.name}
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={() => createDeck.mutate()}
          disabled={createDeck.isPending}
          className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
        >
          <Plus size={14} /> Nytt deck
        </button>
      </header>

      <div className="flex-1 overflow-y-auto p-6">
        {decksQuery.isLoading && (
          <p className="text-sm text-[var(--color-fg-muted)]">Laster decks…</p>
        )}
        {!decksQuery.isLoading && decks.length === 0 && (
          <EmptyState onCreate={() => createDeck.mutate()} />
        )}
        {decks.length > 0 && (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
            {decks.map((deck) => (
              <li key={deck.id}>
                <div className="group relative flex flex-col gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-4 transition-colors hover:border-[var(--color-accent)]/40">
                  <button
                    type="button"
                    onClick={() => setOpenDeck(deck)}
                    className="flex items-center gap-3 text-left"
                  >
                    <span className="grid h-10 w-10 place-items-center rounded-md bg-[var(--color-bg-surface)] text-[var(--color-accent)]">
                      <LayoutTemplate size={18} />
                    </span>
                    <span className="font-medium">{deck.name}</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => deleteDeck.mutate(deck.id)}
                    title="Slett deck"
                    className="absolute right-2 top-2 grid h-7 w-7 place-items-center rounded-md text-[var(--color-fg-muted)] opacity-0 transition-opacity hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-danger)] group-hover:opacity-100"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="mx-auto max-w-md py-16 text-center">
      <div className="mx-auto mb-4 grid h-12 w-12 place-items-center rounded-xl bg-[var(--color-bg-surface)] text-[var(--color-fg-muted)]">
        <LayoutTemplate size={20} />
      </div>
      <h2 className="text-[var(--text-ui-lg)] font-semibold">
        Ingen decks enda
      </h2>
      <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
        Et deck er en samling lysbilder du designer selv — kunngjøringer,
        velkomstskjerm, prekenpunkter.
      </p>
      <button
        type="button"
        onClick={onCreate}
        className="mt-5 inline-flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-4 py-2 text-sm font-medium text-white hover:brightness-110"
      >
        <Plus size={14} /> Lag ditt første deck
      </button>
    </div>
  );
}
