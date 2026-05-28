import {
  LayoutDashboard,
  Library,
  LayoutTemplate,
  CalendarDays,
  BookOpen,
  Image as ImageIcon,
  Settings,
  Play,
} from "lucide-react";

import { cn } from "@/lib/cn";
import { useT, type TKey } from "@/lib/i18n";

type Route =
  | "library"
  | "decks"
  | "services"
  | "bible"
  | "media"
  | "settings"
  | "dashboard";

interface SidebarProps {
  current: Route;
  onNavigate: (route: Route) => void;
  onGoLive: () => void;
}

const NAV_ITEMS: Array<{ id: Route; tkey: TKey; icon: typeof Library }> = [
  { id: "dashboard", tkey: "navDashboard", icon: LayoutDashboard },
  { id: "library",   tkey: "navLibrary",   icon: Library },
  { id: "decks",     tkey: "navDecks",     icon: LayoutTemplate },
  { id: "services",  tkey: "navServices",  icon: CalendarDays },
  { id: "bible",     tkey: "navBible",     icon: BookOpen },
  { id: "media",     tkey: "navMedia",     icon: ImageIcon },
];

export function Sidebar({ current, onNavigate, onGoLive }: SidebarProps) {
  const t = useT();
  return (
    <nav className="flex h-full w-60 flex-col border-r border-[var(--color-border)] bg-[var(--color-bg-elevated)]">
      {/* Brand */}
      <div className="flex items-center gap-2.5 px-4 py-5">
        <div className="grid h-8 w-8 place-items-center rounded-lg bg-[var(--color-brand)] text-[var(--color-accent)] font-bold">
          S
        </div>
        <div className="leading-tight">
          <div className="text-sm font-semibold">SundayStage</div>
          <div className="text-[10px] text-[var(--color-fg-muted)] uppercase tracking-wider">
            {t("appTagline")}
          </div>
        </div>
      </div>

      {/* Nav */}
      <ul className="flex-1 px-2 space-y-0.5">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const isActive = current === item.id;
          return (
            <li key={item.id}>
              <button
                type="button"
                onClick={() => onNavigate(item.id)}
                className={cn(
                  "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                  isActive
                    ? "bg-[var(--color-bg-surface)] text-[var(--color-fg)]"
                    : "text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]/60 hover:text-[var(--color-fg)]",
                )}
              >
                <Icon size={16} aria-hidden />
                <span>{t(item.tkey)}</span>
              </button>
            </li>
          );
        })}
      </ul>

      {/* Bottom */}
      <div className="space-y-2 border-t border-[var(--color-border)] p-3">
        <button
          type="button"
          onClick={() => onNavigate("settings")}
          className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]/60 hover:text-[var(--color-fg)]"
        >
          <Settings size={16} aria-hidden />
          <span>{t("navSettings")}</span>
        </button>
        <button
          type="button"
          onClick={onGoLive}
          className="flex w-full items-center justify-center gap-2 rounded-lg bg-[var(--color-accent)] px-3 py-2.5 text-sm font-bold text-[var(--color-sunday-blue-900)] shadow-sm transition-all hover:brightness-110 active:translate-y-px"
        >
          <Play size={16} aria-hidden fill="currentColor" />
          <span>{t("goLive")}</span>
        </button>
      </div>
    </nav>
  );
}

export type { Route };
