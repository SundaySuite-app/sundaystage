/**
 * Paste-a-plan → preview cue list.
 *
 * The lightweight, non-destructive counterpart to the DB-backed "Import from
 * SundayPlan" file flow. The operator pastes a SundayPlan `ServicePlan` JSON
 * (the handoff payload they already hold — this NEVER fetches it over the
 * network), and we run it through the pure `mapPlanToCues` adapter, resolving
 * each song item against Stage's OWN song/arrangement catalogue via the same IPC
 * the queue editor uses. The result is a preview of the cue list the plan would
 * produce — no rows are written.
 *
 * This is the UI seam around `previewPlanImport`; it closes the gap that nothing
 * built the real `songsByItem` map from Stage's catalogue or called the adapter.
 */
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { AlertTriangle, ClipboardPaste, ListChecks } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type { Library } from "@/lib/bindings";
import { useT } from "@/lib/i18n";
import {
  previewPlanImport,
  type CatalogueResolver,
} from "@/lib/planImportPreview";
import type { MappedItem, PlanServicePlan } from "@/lib/planToStage";
import { Button, Dialog, Textarea } from "@/components/ui";

/**
 * Resolve plan song items against Stage's local catalogue over IPC: match a song
 * by title (the same key the Rust importer uses, since SundayPlan ids don't
 * exist locally), then list its arrangements. Best-effort: any IPC failure for
 * one item resolves it to `null`, so the adapter keeps it as a placeholder
 * rather than failing the whole preview.
 */
function ipcCatalogueResolver(libraryId: string): CatalogueResolver {
  return {
    async resolveSong(item) {
      const title = (item.song_ref?.title ?? item.title ?? "").trim();
      if (!title) return null;
      try {
        const hits = await ipc.song.search(libraryId, title, 5);
        // Prefer an exact (case-insensitive) title match; else the top hit.
        const exact = hits.find(
          (h) => h.title.trim().toLowerCase() === title.toLowerCase(),
        );
        const hit = exact ?? hits[0];
        if (!hit) return null;
        const arrangements = await ipc.arrangement.list(hit.song_id);
        return {
          songId: hit.song_id,
          title: hit.title,
          arrangements: arrangements.map((a) => ({
            id: a.id,
            name: a.name,
            isDefault: a.is_default !== BigInt(0),
          })),
        };
      } catch {
        return null;
      }
    },
  };
}

export function PlanPreviewModal({
  library,
  open,
  onClose,
}: {
  library: Library;
  open: boolean;
  onClose: () => void;
}) {
  const t = useT();
  const [json, setJson] = useState("");

  const preview = useMutation({
    mutationFn: async () => {
      let plan: PlanServicePlan;
      try {
        plan = JSON.parse(json) as PlanServicePlan;
      } catch (e) {
        throw new Error(t("planPreviewInvalidJson", { error: String(e) }), {
          cause: e,
        });
      }
      if (!plan || !Array.isArray(plan.items)) {
        throw new Error(t("planPreviewNoItems"));
      }
      // Defensive default for a payload missing the service envelope: the pure
      // adapter only needs `service.id` for cue namespacing.
      if (!plan.service) plan = { ...plan, service: { id: "pasted-plan" } };
      return previewPlanImport(plan, ipcCatalogueResolver(library.id));
    },
  });

  function reset() {
    preview.reset();
    setJson("");
    onClose();
  }

  const result = preview.data ?? null;
  const fallbackCount =
    result?.items.filter((i) => i.arrangementFallback).length ?? 0;

  return (
    <Dialog
      open={open}
      onClose={reset}
      title={t("planPreviewTitle")}
      description={t("planPreviewDescription")}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" onClick={reset}>
            {t("actionClose")}
          </Button>
          <Button
            onClick={() => preview.mutate()}
            disabled={!json.trim() || preview.isPending}
          >
            <ListChecks size={14} aria-hidden />
            {preview.isPending
              ? t("planPreviewBuilding")
              : t("planPreviewBuild")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex items-center gap-1.5 text-xs text-[var(--color-fg-muted)]">
          <ClipboardPaste size={13} aria-hidden />
          {t("planPreviewPasteLabel")}
        </label>
        <Textarea
          value={json}
          onChange={(e) => setJson(e.target.value)}
          rows={6}
          placeholder={t("planPreviewPastePlaceholder")}
          className="font-mono text-xs"
          spellCheck={false}
        />

        {preview.isError && (
          <p className="flex items-start gap-1.5 rounded-md bg-[var(--color-danger)]/10 px-3 py-2 text-xs text-[var(--color-danger)]">
            <AlertTriangle size={13} aria-hidden className="mt-0.5 shrink-0" />
            <span>{String(preview.error)}</span>
          </p>
        )}

        {result && (
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2 text-xs text-[var(--color-fg-muted)]">
              <span>
                {t(
                  result.cues.length === 1
                    ? "planPreviewCueCountOne"
                    : "planPreviewCueCountMany",
                  { n: result.cues.length },
                )}
              </span>
              {fallbackCount > 0 && (
                <span className="inline-flex items-center gap-1 rounded bg-[var(--color-accent)]/15 px-1.5 py-0.5 text-[var(--color-accent-fg)]">
                  <AlertTriangle size={11} aria-hidden />
                  {t("planPreviewFallbacks", { n: fallbackCount })}
                </span>
              )}
            </div>
            <ol className="max-h-72 space-y-1 overflow-y-auto">
              {result.items.map((item, i) => (
                <PreviewRow key={item.cue.cue_id} index={i} item={item} />
              ))}
            </ol>
          </div>
        )}
      </div>
    </Dialog>
  );
}

function PreviewRow({ index, item }: { index: number; item: MappedItem }) {
  const t = useT();
  const label =
    item.cue.kind === "show_slide"
      ? (item.cue.source.display_label ??
        item.cue.slide_content.reference ??
        item.stageKind)
      : item.cue.kind === "pause"
        ? item.cue.label
        : item.stageKind;

  return (
    <li className="flex items-center gap-2 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-3 py-1.5 text-sm">
      <span className="w-5 shrink-0 text-right font-mono text-xs text-[var(--color-fg-muted)] tabular-nums">
        {index + 1}
      </span>
      <span className="rounded bg-[var(--color-bg-surface)] px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-[var(--color-fg-muted)] uppercase">
        {item.stageKind}
      </span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {item.arrangementFallback && (
        <span
          className="inline-flex shrink-0 items-center gap-1 rounded bg-[var(--color-accent)]/15 px-1.5 py-0.5 text-[10px] text-[var(--color-accent-fg)]"
          title={t("planPreviewFallbackHint")}
        >
          <AlertTriangle size={10} aria-hidden />
          {t("planPreviewFallbackBadge")}
        </span>
      )}
    </li>
  );
}
