/**
 * The schedule rail — the EasyWorship-style spine on the far left. It shows the
 * selected service top-to-bottom the way the operator runs it: every item, its
 * kind and slide count. Clicking an item focuses it (stages its first slide in
 * Preview and scrolls the grid). Fast reorder/remove live here; richer editing
 * (add song with arrangement + key, scripture, notes, rename, date) opens the
 * full schedule editor — we reuse the existing ServicesPage wholesale for that.
 */
import { useMemo } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  BookOpen,
  CalendarDays,
  Music,
  Pause,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { Service } from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT, useLocale, type TKey } from "@/lib/i18n";
import { localizeSectionLabel } from "@/lib/sectionLabel";

const KIND_ICON: Record<string, typeof Music> = {
  song: Music,
  scripture: BookOpen,
  gap: Pause,
  announcement: CalendarDays,
};
const KIND_KEY: Record<string, TKey> = {
  song: "kindSong",
  scripture: "kindScripture",
  custom_deck: "kindCustomDeck",
  gap: "kindGap",
  announcement: "kindAnnouncement",
  video: "kindVideo",
};

interface Props {
  service: Service;
  /** service_item_id of the item currently focused in the grid. */
  focusedItemId: string | null;
  onFocusItem: (serviceItemId: string) => void;
  onEditSchedule: () => void;
}

export function ScheduleRail({
  service,
  focusedItemId,
  onFocusItem,
  onEditSchedule,
}: Props) {
  const t = useT();
  const lang = useLocale((s) => s.lang);
  const qc = useQueryClient();

  const summaryQuery = useQuery({
    queryKey: ["cueSummary", service.id],
    queryFn: () => ipc.service.cueSummary(service.id),
  });
  const items = useMemo(
    () => summaryQuery.data?.items ?? [],
    [summaryQuery.data],
  );

  function refresh() {
    void qc.invalidateQueries({ queryKey: ["cueSummary", service.id] });
    void qc.invalidateQueries({ queryKey: ["cueList", service.id] });
    void qc.invalidateQueries({ queryKey: ["serviceItems", service.id] });
  }

  const reorder = useMutation({
    mutationFn: (orderedIds: string[]) =>
      ipc.service.reorderItems(service.id, orderedIds),
    onSuccess: refresh,
  });
  const removeItem = useMutation({
    mutationFn: (itemId: string) => ipc.service.removeItem(itemId),
    onSuccess: refresh,
  });

  function move(index: number, dir: -1 | 1) {
    const to = index + dir;
    if (to < 0 || to >= items.length) return;
    const ids = items.map((i) => i.service_item_id);
    const [moved] = ids.splice(index, 1);
    ids.splice(to, 0, moved);
    reorder.mutate(ids);
  }

  return (
    <aside className="flex h-full min-h-0 flex-col border-r border-[var(--color-border)] bg-[var(--color-bg-elevated)]">
      <header className="flex items-center gap-2 border-b border-[var(--color-border)] px-3 py-2.5">
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-semibold">{service.name}</h2>
          <p className="text-[11px] text-[var(--color-fg-muted)]">
            {formatDate(Number(service.starts_at), lang)} ·{" "}
            {summaryQuery.data
              ? t("svcCuesInQueue", { n: summaryQuery.data.total_cues })
              : t("loadingShort")}
          </p>
        </div>
        <button
          type="button"
          onClick={onEditSchedule}
          title={t("wsEditSchedule")}
          className="rounded-md p-1.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          <Pencil size={15} aria-hidden />
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {items.length === 0 ? (
          <p className="px-3 py-6 text-center text-sm text-[var(--color-fg-muted)]">
            {t("wsScheduleEmpty")}
          </p>
        ) : (
          <ol className="space-y-1">
            {items.map((item, i) => {
              const Icon = KIND_ICON[item.kind] ?? Music;
              const focused = item.service_item_id === focusedItemId;
              return (
                <li key={item.service_item_id} className="group/row">
                  <div
                    className={cn(
                      "flex items-start gap-2 rounded-md px-2 py-1.5 transition-colors",
                      focused
                        ? "bg-[var(--color-accent)]/15 ring-1 ring-[var(--color-accent)]"
                        : "hover:bg-[var(--color-bg-surface)]",
                    )}
                  >
                    <button
                      type="button"
                      onClick={() => onFocusItem(item.service_item_id)}
                      className="flex min-w-0 flex-1 items-start gap-2 text-left"
                    >
                      <span className="mt-0.5 w-4 text-center font-mono text-[11px] text-[var(--color-fg-muted)] tabular-nums">
                        {i + 1}
                      </span>
                      <Icon
                        size={14}
                        aria-hidden
                        className="mt-0.5 shrink-0 text-[var(--color-fg-muted)]"
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm font-medium">
                          {item.title}
                        </span>
                        <span className="block text-[11px] text-[var(--color-fg-muted)]">
                          {KIND_KEY[item.kind]
                            ? t(KIND_KEY[item.kind])
                            : item.kind}
                          {" · "}
                          {t(
                            item.cue_count === 1
                              ? "slideCountOne"
                              : "slideCountMany",
                            { n: item.cue_count },
                          )}
                        </span>
                        {item.sections.length > 0 && (
                          <span className="mt-1 flex flex-wrap gap-1">
                            {item.sections.slice(0, 6).map((sec, si) => (
                              <span
                                key={si}
                                className="rounded bg-[var(--color-bg-surface)] px-1 py-0.5 text-[10px] text-[var(--color-fg-muted)]"
                              >
                                {localizeSectionLabel(sec.label, t)}
                              </span>
                            ))}
                          </span>
                        )}
                      </span>
                    </button>
                    <span className="flex shrink-0 flex-col gap-0.5 opacity-0 transition-opacity group-hover/row:opacity-100">
                      <button
                        type="button"
                        onClick={() => move(i, -1)}
                        disabled={i === 0}
                        title={t("svcMoveUp")}
                        className="rounded p-0.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-elevated)] hover:text-[var(--color-fg)] disabled:opacity-30"
                      >
                        <ArrowUp size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => move(i, 1)}
                        disabled={i === items.length - 1}
                        title={t("svcMoveDown")}
                        className="rounded p-0.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-elevated)] hover:text-[var(--color-fg)] disabled:opacity-30"
                      >
                        <ArrowDown size={12} />
                      </button>
                      <button
                        type="button"
                        onClick={() => removeItem.mutate(item.service_item_id)}
                        title={t("svcRemoveFromQueue")}
                        className="rounded p-0.5 text-[var(--color-fg-muted)] hover:bg-[var(--color-danger)]/15 hover:text-[var(--color-danger)]"
                      >
                        <Trash2 size={12} />
                      </button>
                    </span>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </div>

      <div className="border-t border-[var(--color-border)] p-2">
        <button
          type="button"
          onClick={onEditSchedule}
          className="flex w-full items-center justify-center gap-1.5 rounded-md border border-dashed border-[var(--color-border)] px-3 py-2 text-sm text-[var(--color-fg-muted)] hover:border-[var(--color-accent)] hover:text-[var(--color-fg)]"
        >
          <Plus size={14} aria-hidden />
          {t("wsAddToSchedule")}
        </button>
      </div>
    </aside>
  );
}

function formatDate(ms: number, lang: string): string {
  return new Date(ms).toLocaleDateString(lang, {
    weekday: "short",
    day: "numeric",
    month: "short",
  });
}
