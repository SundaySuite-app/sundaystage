/**
 * ⌘K command palette — keyboard-first navigation, actions, and universal
 * search (Phase 2.3).
 *
 * Short input → navigation + quick actions. Two+ characters → live results
 * across songs, Bible, and services from the FTS-backed `search_all` command,
 * grouped by type. Selecting a result jumps to that section.
 */

import { Command } from "cmdk";
import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Library,
  LayoutTemplate,
  CalendarDays,
  Play,
  Plus,
  BookOpen,
  Image as ImageIcon,
  Settings,
  Palette,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import { useT } from "@/lib/i18n";

/** Navigation targets the palette can jump to. The operator workspace maps
 *  these onto its overlays (library browser tab, media drawer, settings). */
export type Route =
  | "library"
  | "decks"
  | "services"
  | "bible"
  | "media"
  | "settings"
  | "dashboard"
  | "design";

interface CommandPaletteProps {
  onNavigate: (route: Route) => void;
  /** Open a specific search hit (song/service) rather than just its page. */
  onOpenResult?: (route: Route, id: string) => void;
  libraryId?: string | null;
}

const KIND_ROUTE: Record<string, Route> = {
  song: "library",
  bible: "bible",
  service: "services",
};

export function CommandPalette({
  onNavigate,
  onOpenResult,
  libraryId,
}: CommandPaletteProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");

  // ⌘K / Ctrl+K toggles the palette
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setOpen((prev) => !prev);
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const searching = q.trim().length > 1;
  const results = useQuery({
    queryKey: ["universalSearch", libraryId, q],
    queryFn: () => ipc.search.all(libraryId!, q),
    enabled: open && searching && !!libraryId,
  });

  function close() {
    setOpen(false);
    setQ("");
  }
  function go(route: Route) {
    onNavigate(route);
    close();
  }
  function openHit(route: Route, id: string) {
    if (onOpenResult) onOpenResult(route, id);
    else onNavigate(route);
    close();
  }

  if (!open) return null;

  const hits = results.data ?? [];
  const songs = hits.filter((h) => h.kind === "song");
  const bible = hits.filter((h) => h.kind === "bible");
  const services = hits.filter((h) => h.kind === "service");

  return (
    <Command.Dialog
      open
      onOpenChange={(o) => (o ? setOpen(true) : close())}
      label={t("cmdPaletteLabel")}
      shouldFilter={!searching}
      className="fixed inset-0 z-50 grid place-items-start pt-[12vh]"
    >
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={close}
        aria-hidden
      />

      <div className="relative mx-auto w-full max-w-2xl overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <Command.Input
          autoFocus
          value={q}
          onValueChange={setQ}
          placeholder={t("cmdSearchPlaceholder")}
          className="w-full border-b border-[var(--color-border)] bg-transparent px-4 py-3 text-[var(--text-ui-md)] text-[var(--color-fg)] placeholder:text-[var(--color-fg-muted)] focus:outline-none"
        />
        <Command.List className="max-h-[60vh] overflow-y-auto p-2">
          {searching ? (
            <>
              {results.isLoading && (
                <div className="px-3 py-6 text-center text-sm text-[var(--color-fg-muted)]">
                  {t("cmdSearching")}
                </div>
              )}
              {!results.isLoading && hits.length === 0 && (
                <div className="px-3 py-6 text-center text-sm text-[var(--color-fg-muted)]">
                  {t("cmdNoHits", { q })}
                </div>
              )}
              {songs.length > 0 && (
                <Group heading={t("groupSongs")}>
                  {songs.map((h) => (
                    <ResultItem
                      key={h.id}
                      icon={<Library size={14} />}
                      title={h.title}
                      subtitle={h.subtitle}
                      onSelect={() => openHit(KIND_ROUTE.song, h.id)}
                    />
                  ))}
                </Group>
              )}
              {bible.length > 0 && (
                <Group heading={t("groupBible")}>
                  {bible.map((h) => (
                    <ResultItem
                      key={h.id}
                      icon={<BookOpen size={14} />}
                      title={h.title}
                      subtitle={h.subtitle}
                      onSelect={() => go(KIND_ROUTE.bible)}
                    />
                  ))}
                </Group>
              )}
              {services.length > 0 && (
                <Group heading={t("groupServices")}>
                  {services.map((h) => (
                    <ResultItem
                      key={h.id}
                      icon={<CalendarDays size={14} />}
                      title={h.title}
                      subtitle={h.subtitle}
                      onSelect={() => openHit(KIND_ROUTE.service, h.id)}
                    />
                  ))}
                </Group>
              )}
            </>
          ) : (
            <>
              <Group heading={t("cmdGroupNavigate")}>
                <Item
                  onSelect={() => go("library")}
                  icon={<Library size={14} />}
                  label={t("cmdSongLibrary")}
                />
                <Item
                  onSelect={() => go("decks")}
                  icon={<LayoutTemplate size={14} />}
                  label={t("navDecks")}
                />
                <Item
                  onSelect={() => go("services")}
                  icon={<CalendarDays size={14} />}
                  label={t("navServices")}
                />
                <Item
                  onSelect={() => go("bible")}
                  icon={<BookOpen size={14} />}
                  label={t("navBible")}
                />
                <Item
                  onSelect={() => go("media")}
                  icon={<ImageIcon size={14} />}
                  label={t("navMedia")}
                />
                <Item
                  onSelect={() => go("settings")}
                  icon={<Settings size={14} />}
                  label={t("navSettings")}
                />
              </Group>

              <Group heading={t("cmdGroupActions")}>
                <Item
                  onSelect={() => {}}
                  icon={<Plus size={14} />}
                  label={t("cmdNewSong")}
                  shortcut="N"
                />
                <Item
                  onSelect={() => {}}
                  icon={<Plus size={14} />}
                  label={t("cmdNewService")}
                  shortcut="⌘N"
                />
                <Item
                  onSelect={() => {}}
                  icon={<Play size={14} fill="currentColor" />}
                  label={t("goLive")}
                  shortcut="⌘L"
                />
              </Group>

              {import.meta.env.DEV && (
                <Group heading={t("cmdGroupDeveloper")}>
                  <Item
                    onSelect={() => go("design")}
                    icon={<Palette size={14} />}
                    label={t("cmdDesignSystem")}
                  />
                </Group>
              )}
            </>
          )}
        </Command.List>
      </div>
    </Command.Dialog>
  );
}

function Group({
  heading,
  children,
}: {
  heading: string;
  children: React.ReactNode;
}) {
  return (
    <Command.Group
      heading={heading}
      className="mt-2 mb-1 px-2 text-xs font-medium tracking-wider text-[var(--color-fg-muted)] uppercase"
    >
      {children}
    </Command.Group>
  );
}

function Item({
  onSelect,
  icon,
  label,
  shortcut,
}: {
  onSelect: () => void;
  icon: React.ReactNode;
  label: string;
  shortcut?: string;
}) {
  return (
    <Command.Item
      onSelect={onSelect}
      className="flex cursor-pointer items-center gap-2.5 rounded-md px-3 py-2 text-sm text-[var(--color-fg)] aria-selected:bg-[var(--color-bg-surface)]"
    >
      <span className="text-[var(--color-fg-muted)]">{icon}</span>
      <span className="flex-1">{label}</span>
      {shortcut ? (
        <kbd className="rounded border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--color-fg-muted)]">
          {shortcut}
        </kbd>
      ) : null}
    </Command.Item>
  );
}

function ResultItem({
  onSelect,
  icon,
  title,
  subtitle,
}: {
  onSelect: () => void;
  icon: React.ReactNode;
  title: string;
  subtitle: string;
}) {
  return (
    <Command.Item
      onSelect={onSelect}
      className="flex cursor-pointer items-center gap-2.5 rounded-md px-3 py-2 text-sm text-[var(--color-fg)] aria-selected:bg-[var(--color-bg-surface)]"
    >
      <span className="text-[var(--color-fg-muted)]">{icon}</span>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate">{title}</span>
        {subtitle && (
          <span className="truncate text-[11px] text-[var(--color-fg-muted)]">
            {subtitle}
          </span>
        )}
      </span>
    </Command.Item>
  );
}
