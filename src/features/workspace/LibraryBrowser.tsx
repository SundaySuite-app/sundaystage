/**
 * The resource browser — ProPresenter's library + FreeShow's tabbed drawer,
 * DOCKED as a left panel instead of an overlay: the console (grid, Preview,
 * Program, transport) stays visible and its hotkeys stay armed while the
 * operator finds a song, scripture, deck or theme. `data-console-dock` marks
 * the subtree for the workspace's keyboard scoping (see consoleKeys.ts):
 * navigation keys stay local in here, panic keys (B/L/Esc) still reach the
 * console. Each tab reuses the existing full-page feature wholesale, so all
 * their behaviour (search, editing, AI formatting, deep-link) comes for free.
 */
import { useEffect, useState } from "react";
import {
  BookOpen,
  Library as LibraryIcon,
  LayoutTemplate,
  Palette,
  X,
} from "lucide-react";

import type { Library, ServiceItem } from "@/lib/bindings";
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
  /** Workspace context for the Scripture tab's add/show-now actions. */
  activeService?: { id: string; name: string } | null;
  isLive?: boolean;
  onBibleAdded?: (
    item: ServiceItem,
    opts: { showNow: boolean },
  ) => void | Promise<void>;
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
  activeService,
  isLive,
  onBibleAdded,
  onClose,
}: Props) {
  const t = useT();
  const [tab, setTab] = useState<BrowserTab>(initialTab);

  useEffect(() => {
    if (open) setTab(initialTab);
  }, [open, initialTab]);

  // Esc is handled by the workspace's scoped key handler (close the dock,
  // otherwise blackout) — no listener of our own, it would double-fire.

  if (!open) return null;

  return (
    <div
      data-console-dock
      className="flex h-full w-[clamp(380px,38vw,620px)] shrink-0 flex-col border-r border-[var(--color-border)] bg-[var(--color-bg)]"
    >
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
              activeService={activeService}
              isLive={isLive}
              onAdded={onBibleAdded}
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
  );
}
