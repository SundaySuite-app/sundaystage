/**
 * The resource browser — ProPresenter's library + FreeShow's tabbed drawer.
 * Slides in from the left over the workspace so the operator can find a song,
 * scripture, deck or theme without leaving the console. Each tab reuses the
 * existing full-page feature wholesale, so all their behaviour (search,
 * editing, AI formatting, deep-link) comes along for free.
 */
import { useEffect, useState } from "react";
import {
  BookOpen,
  Library as LibraryIcon,
  LayoutTemplate,
  Palette,
  X,
} from "lucide-react";

import type { Library } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT, type TKey } from "@/lib/i18n";
import { LibraryPage } from "@/features/library/LibraryPage";
import { BiblePage, type BibleDeepLink } from "@/features/bible/BiblePage";
import { DecksPage } from "@/features/decks/DecksPage";

export type BrowserTab = "songs" | "scripture" | "decks" | "themes";

const TABS: Array<{ id: BrowserTab; tkey: TKey; icon: typeof LibraryIcon }> = [
  { id: "songs", tkey: "navLibrary", icon: LibraryIcon },
  { id: "scripture", tkey: "navBible", icon: BookOpen },
  { id: "decks", tkey: "navDecks", icon: LayoutTemplate },
  { id: "themes", tkey: "wsThemesTab", icon: Palette },
];

interface Props {
  library: Library;
  open: boolean;
  initialTab?: BrowserTab;
  /** Deep-link a song open in the Songs tab. */
  openSongId?: string | null;
  onDeepLinkDone?: () => void;
  /** Deep-link a bible passage open in the Scripture tab. */
  bibleDeepLink?: BibleDeepLink | null;
  onBibleDeepLinkDone?: () => void;
  onClose: () => void;
}

export function LibraryBrowser({
  library,
  open,
  initialTab = "songs",
  openSongId,
  onDeepLinkDone,
  bibleDeepLink,
  onBibleDeepLinkDone,
  onClose,
}: Props) {
  const t = useT();
  const [tab, setTab] = useState<BrowserTab>(initialTab);

  useEffect(() => {
    if (open) setTab(initialTab);
  }, [open, initialTab]);

  // Esc closes the browser.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-40 flex">
      <div
        className="absolute inset-0 bg-black/40"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative flex h-full w-[min(92vw,1100px)] flex-col border-r border-[var(--color-border)] bg-[var(--color-bg)] shadow-[var(--shadow-elevated)]">
        <div className="flex items-center gap-1 border-b border-[var(--color-border)] px-3 py-2">
          {TABS.map(({ id, tkey, icon: Icon }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={cn(
                "flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                tab === id
                  ? "bg-[var(--color-bg-surface)] text-[var(--color-fg)]"
                  : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]/60 hover:text-[var(--color-fg)]",
              )}
            >
              <Icon size={15} aria-hidden />
              {t(tkey)}
            </button>
          ))}
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            title={t("actionClose")}
            className="rounded-md p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={16} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-hidden">
          {tab === "songs" ? (
            <LibraryPage
              library={library}
              openSongId={openSongId ?? null}
              onDeepLinkDone={onDeepLinkDone}
            />
          ) : tab === "scripture" ? (
            <BiblePage
              library={library}
              deepLink={bibleDeepLink}
              onDeepLinkDone={onBibleDeepLinkDone}
            />
          ) : tab === "decks" ? (
            <DecksPage library={library} />
          ) : (
            <div className="grid h-full place-items-center p-10 text-center">
              <div className="max-w-sm">
                <Palette
                  size={28}
                  className="mx-auto mb-3 text-[var(--color-fg-muted)]"
                  aria-hidden
                />
                <h3 className="text-[var(--text-ui-lg)] font-semibold">
                  {t("wsThemesTab")}
                </h3>
                <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
                  {t("wsThemesComingSoon")}
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
