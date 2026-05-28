/**
 * ⌘K command palette — keyboard-first navigation + actions.
 *
 * Currently surfaces:
 *   - Page navigation
 *   - Quick actions (new song, new service, ...)
 *
 * Phase 2.3 adds search results (songs, services, bible) feeding in
 * from the FTS5-backed Rust search.
 */

import { Command } from "cmdk";
import { useEffect, useState } from "react";
import {
  Library,
  LayoutTemplate,
  CalendarDays,
  Play,
  Plus,
  BookOpen,
  Image as ImageIcon,
  Settings,
} from "lucide-react";

import type { Route } from "./Sidebar";

interface CommandPaletteProps {
  onNavigate: (route: Route) => void;
}

export function CommandPalette({ onNavigate }: CommandPaletteProps) {
  const [open, setOpen] = useState(false);

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

  function go(route: Route) {
    onNavigate(route);
    setOpen(false);
  }

  if (!open) return null;

  return (
    <Command.Dialog
      open
      onOpenChange={setOpen}
      label="Kommandopalett"
      className="fixed inset-0 z-50 grid place-items-start pt-[12vh]"
    >
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/40 backdrop-blur-sm"
        onClick={() => setOpen(false)}
        aria-hidden
      />

      <div className="relative w-full max-w-2xl mx-auto overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <Command.Input
          autoFocus
          placeholder="Søk etter sanger, tjenester, eller skriv en kommando…"
          className="w-full border-b border-[var(--color-border)] bg-transparent px-4 py-3 text-[var(--text-ui-md)] text-[var(--color-fg)] placeholder:text-[var(--color-fg-muted)] focus:outline-none"
        />
        <Command.List className="max-h-[60vh] overflow-y-auto p-2">
          <Command.Empty className="px-3 py-6 text-center text-sm text-[var(--color-fg-muted)]">
            Ingen treff.
          </Command.Empty>

          <Command.Group
            heading="Naviger"
            className="text-xs font-medium uppercase tracking-wider text-[var(--color-fg-muted)] mb-1 mt-2 px-2"
          >
            <Item
              onSelect={() => go("library")}
              icon={<Library size={14} />}
              label="Sangbibliotek"
            />
            <Item
              onSelect={() => go("decks")}
              icon={<LayoutTemplate size={14} />}
              label="Decks"
            />
            <Item
              onSelect={() => go("services")}
              icon={<CalendarDays size={14} />}
              label="Tjenester"
            />
            <Item
              onSelect={() => go("bible")}
              icon={<BookOpen size={14} />}
              label="Bibel"
            />
            <Item
              onSelect={() => go("media")}
              icon={<ImageIcon size={14} />}
              label="Media"
            />
            <Item
              onSelect={() => go("settings")}
              icon={<Settings size={14} />}
              label="Innstillinger"
            />
          </Command.Group>

          <Command.Group
            heading="Handlinger"
            className="text-xs font-medium uppercase tracking-wider text-[var(--color-fg-muted)] mb-1 mt-4 px-2"
          >
            <Item
              onSelect={() => {}}
              icon={<Plus size={14} />}
              label="Ny sang…"
              shortcut="N"
            />
            <Item
              onSelect={() => {}}
              icon={<Plus size={14} />}
              label="Ny tjeneste…"
              shortcut="⌘N"
            />
            <Item
              onSelect={() => {}}
              icon={<Play size={14} fill="currentColor" />}
              label="Gå live"
              shortcut="⌘L"
            />
          </Command.Group>
        </Command.List>
      </div>
    </Command.Dialog>
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
