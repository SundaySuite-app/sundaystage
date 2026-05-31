/**
 * The top transport bar — the one strip that's always present, like every
 * worship console. Left: browse + brand + service picker. Center: the live
 * transport (Black / Logo) and the on-air state. Right: jump, stage screen,
 * export, output assignment, settings, theme and sync.
 *
 * We only surface transport actions the Rust engine actually performs
 * (blackout, show_logo) — no decorative buttons. They arm-gate on a live
 * session: until you press Go Live they're disabled.
 */
import { useQuery } from "@tanstack/react-query";
import {
  Clapperboard,
  Menu,
  Monitor,
  Play,
  Search,
  Settings as SettingsIcon,
  Square,
  SquareDot,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { OutputState, Service, SyncStatus } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT, useLocale, type TKey } from "@/lib/i18n";
import { ThemeToggle } from "@/components/ThemeToggle";
import { OutputControls } from "@/features/live/OutputControls";

interface Props {
  services: Service[];
  service: Service | null;
  onSelectService: (id: string) => void;
  onNewService: () => void;
  onOpenBrowser: () => void;
  isLive: boolean;
  canGoLive: boolean;
  outputState: OutputState | null;
  onGoLive: () => void;
  onStop: () => void;
  onBlackout: () => void;
  onLogo: () => void;
  onJump: () => void;
  onStage: () => void;
  onExport: () => void;
  onSettings: () => void;
}

export function TransportBar({
  services,
  service,
  onSelectService,
  onNewService,
  onOpenBrowser,
  isLive,
  canGoLive,
  outputState,
  onGoLive,
  onStop,
  onBlackout,
  onLogo,
  onJump,
  onStage,
  onExport,
  onSettings,
}: Props) {
  const t = useT();
  const lang = useLocale((s) => s.lang);

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[var(--color-border)] bg-[var(--color-console)] px-3 text-sm">
      {/* Left: browse + brand + service */}
      <button
        type="button"
        onClick={onOpenBrowser}
        title={t("wsBrowseLibrary")}
        className="flex items-center gap-2 rounded-md px-2 py-1.5 text-[var(--color-fg-muted)] hover:bg-white/5 hover:text-[var(--color-fg)]"
      >
        <Menu size={16} aria-hidden />
        <span className="hidden sm:inline">{t("wsBrowseLibrary")}</span>
      </button>

      <div className="mx-1 flex items-center gap-2">
        <div className="grid h-7 w-7 place-items-center rounded-md bg-[var(--color-brand)] font-bold text-[var(--color-accent)]">
          S
        </div>
        <select
          value={service?.id ?? ""}
          onChange={(e) => {
            if (e.target.value === "__new__") onNewService();
            else onSelectService(e.target.value);
          }}
          className="max-w-[220px] truncate rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm text-[var(--color-fg)] focus:border-[var(--color-accent)] focus:outline-none"
        >
          {!service && <option value="">{t("wsNoServiceTitle")}</option>}
          {services.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name} · {formatDate(Number(s.starts_at), lang)}
            </option>
          ))}
          <option value="__new__">+ {t("svcNewService")}</option>
        </select>
      </div>

      {/* Center: transport */}
      <div className="mx-auto flex items-center gap-1">
        <TransportButton
          icon={Square}
          labelKey="liveBlackout"
          active={outputState === "blackout"}
          disabled={!isLive}
          onClick={onBlackout}
        />
        <TransportButton
          icon={SquareDot}
          labelKey="liveLogo"
          active={outputState === "logo"}
          disabled={!isLive}
          onClick={onLogo}
        />
        <div className="mx-2 h-6 w-px bg-[var(--color-border)]" />
        {isLive ? (
          <div className="flex items-center gap-2">
            <span className="flex items-center gap-1.5 rounded-md bg-[var(--color-on-air)] px-2.5 py-1.5 text-xs font-bold text-[var(--color-sunday-blue-900)]">
              <span className="h-2 w-2 animate-pulse rounded-full bg-current" />
              {t("wsLiveBadge")}
            </span>
            <button
              type="button"
              onClick={onStop}
              className="rounded-md border border-[var(--color-border)] px-2.5 py-1.5 text-xs font-medium text-[var(--color-fg-muted)] hover:bg-white/5 hover:text-[var(--color-fg)]"
            >
              {t("wsStop")}
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={onGoLive}
            disabled={!canGoLive}
            title={
              canGoLive ? t("svcGoLiveTooltip") : t("svcQueueEmptyTooltip")
            }
            className="flex items-center gap-1.5 rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-xs font-bold text-[var(--color-sunday-blue-900)] transition-all hover:brightness-110 active:translate-y-px disabled:opacity-40"
          >
            <Play size={13} aria-hidden fill="currentColor" />
            {t("goLive")}
          </button>
        )}
      </div>

      {/* Right: tools */}
      <div className="flex items-center gap-1">
        <IconButton
          icon={Search}
          label={`${t("lpJumpTo")} (⌘J)`}
          onClick={onJump}
          disabled={!isLive}
        />
        <IconButton
          icon={Monitor}
          label={t("liveStageScreen")}
          onClick={onStage}
          disabled={!isLive}
        />
        <IconButton
          icon={Clapperboard}
          label={t("lpExport")}
          onClick={onExport}
          disabled={!isLive}
        />
        <div className="mx-1 h-6 w-px bg-[var(--color-border)]" />
        <OutputControls />
        <IconButton
          icon={SettingsIcon}
          label={t("navSettings")}
          onClick={onSettings}
        />
        <ThemeToggle />
        <SyncBadge />
      </div>
    </header>
  );
}

function TransportButton({
  icon: Icon,
  labelKey,
  active,
  disabled,
  onClick,
}: {
  icon: typeof Square;
  labelKey: TKey;
  active: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs font-medium transition-colors disabled:opacity-40",
        active
          ? "bg-[var(--color-accent)]/20 text-[var(--color-accent)] ring-1 ring-[var(--color-accent)]"
          : "text-[var(--color-fg-muted)] hover:bg-white/5 hover:text-[var(--color-fg)]",
      )}
    >
      <Icon size={14} aria-hidden />
      {t(labelKey)}
    </button>
  );
}

function IconButton({
  icon: Icon,
  label,
  onClick,
  disabled,
}: {
  icon: typeof Search;
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={label}
      aria-label={label}
      className="rounded-md p-2 text-[var(--color-fg-muted)] hover:bg-white/5 hover:text-[var(--color-fg)] disabled:opacity-40"
    >
      <Icon size={16} aria-hidden />
    </button>
  );
}

const SYNC_KEY: Record<SyncStatus, TKey> = {
  local_only: "syncLocalOnly",
  synced: "syncSynced",
  syncing: "syncSyncing",
  offline: "syncOffline",
  conflict: "syncConflict",
  paused_live: "syncPausedLive",
};
const SYNC_DOT: Record<SyncStatus, string> = {
  local_only: "bg-[var(--color-fg-muted)]",
  synced: "bg-[var(--color-success)]",
  syncing: "bg-[var(--color-info)]",
  offline: "bg-[var(--color-fg-muted)]",
  conflict: "bg-[var(--color-danger)]",
  paused_live: "bg-[var(--color-warning)]",
};

function SyncBadge() {
  const t = useT();
  const { data } = useQuery({
    queryKey: ["syncStatus"],
    queryFn: () => ipc.sync.status(),
  });
  const status: SyncStatus = data ?? "local_only";
  return (
    <div
      title={t(SYNC_KEY[status])}
      className="ml-1 flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] text-[var(--color-fg-muted)]"
    >
      <span className={cn("h-2 w-2 rounded-full", SYNC_DOT[status])} />
      <span className="hidden lg:inline">{t(SYNC_KEY[status])}</span>
    </div>
  );
}

function formatDate(ms: number, lang: string): string {
  return new Date(ms).toLocaleDateString(lang, {
    day: "numeric",
    month: "short",
  });
}
