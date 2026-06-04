/**
 * Output display configuration panel — technical output settings.
 *
 * Covers: primary display index, target resolution, text safe-zone percentage,
 * and the slide transition (type + duration). Persisted via the Rust
 * `output_display_config` / `output_set_display_config` commands.
 *
 * Separated from `OutputSettings` (which handles visual appearance) so that
 * operators can keep colour/font decisions away from technical AV settings.
 */

import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Monitor } from "lucide-react";

import { ipc } from "@/lib/ipc";
import type {
  OutputDisplayConfig,
  OutputResolution,
  OutputTransition,
} from "@/lib/bindings";
import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";
import { useErrorToast } from "@/lib/useErrorToast";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  ErrorToast,
  Select,
} from "@/components/ui";

const DEFAULT_DISPLAY_CONFIG: OutputDisplayConfig = {
  primary_display_index: 0,
  output_resolution: "1920x1080",
  text_safe_zone_percent: 10,
  transition: "cut",
  transition_ms: 300,
};

const RESOLUTIONS: Array<{ value: OutputResolution; label: string }> = [
  { value: "1920x1080", label: "1920 × 1080 (Full HD)" },
  { value: "1280x720", label: "1280 × 720 (HD)" },
  { value: "3840x2160", label: "3840 × 2160 (4K UHD)" },
];

const TRANSITIONS: Array<{ value: OutputTransition; labelKey: string }> = [
  { value: "cut", labelKey: "setOutDispTransitionCut" },
  { value: "fade", labelKey: "setOutDispTransitionFade" },
  { value: "slide_left", labelKey: "setOutDispTransitionSlideLeft" },
  { value: "slide_right", labelKey: "setOutDispTransitionSlideRight" },
];

export function OutputSettingsPanel() {
  const t = useT();
  const qc = useQueryClient();
  const { message: error, showError, dismiss } = useErrorToast();
  const [draft, setDraft] = useState<OutputDisplayConfig>(
    DEFAULT_DISPLAY_CONFIG,
  );
  const [saved, setSaved] = useState(false);
  const loaded = useRef(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );

  const configQuery = useQuery({
    queryKey: ["outputDisplayConfig"],
    queryFn: () => ipc.output.displayConfig(),
  });

  // Seed draft once the saved config arrives.
  useEffect(() => {
    if (configQuery.data && !loaded.current) {
      loaded.current = true;
      setDraft(configQuery.data);
    }
  }, [configQuery.data]);

  function update(patch: Partial<OutputDisplayConfig>) {
    const next = { ...draft, ...patch };
    setDraft(next);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      void ipc.output
        .setDisplayConfig(next)
        .then((saved) => {
          qc.setQueryData(["outputDisplayConfig"], saved);
          setSaved(true);
          setTimeout(() => setSaved(false), 2000);
        })
        .catch(() => showError(t("setSaveFailed")));
    }, 200);
  }

  const showTransitionMs = draft.transition !== "cut";

  return (
    <>
      <ErrorToast message={error} onDismiss={dismiss} />
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Monitor size={18} className="text-[var(--color-accent)]" />
            {t("setOutDispTitle")}
          </CardTitle>
          <CardDescription>{t("setOutDispDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          {/* Primary display index */}
          <FieldRow
            label={t("setOutDispPrimaryDisplay")}
            description={t("setOutDispPrimaryDisplayDesc")}
          >
            <input
              type="number"
              min={0}
              max={8}
              value={draft.primary_display_index}
              onChange={(e) =>
                update({
                  primary_display_index: Math.max(0, Number(e.target.value)),
                })
              }
              className="w-20 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-sm focus:border-[var(--color-accent)] focus:outline-none"
            />
          </FieldRow>

          {/* Resolution */}
          <FieldRow label={t("setOutDispResolution")}>
            <Select
              className="w-52"
              value={draft.output_resolution}
              onChange={(e) =>
                update({
                  output_resolution: e.target.value as OutputResolution,
                })
              }
            >
              {RESOLUTIONS.map((r) => (
                <option key={r.value} value={r.value}>
                  {r.label}
                </option>
              ))}
            </Select>
          </FieldRow>

          {/* Safe zone */}
          <div>
            <div className="mb-1 flex items-center justify-between text-sm">
              <span className="text-[var(--color-fg-muted)]">
                {t("setOutDispSafeZone")}
              </span>
              <span className="font-mono text-xs text-[var(--color-fg)]">
                {Math.round(draft.text_safe_zone_percent)}%
              </span>
            </div>
            <p className="mb-1 text-xs text-[var(--color-fg-muted)]">
              {t("setOutDispSafeZoneDesc")}
            </p>
            <input
              type="range"
              min={5}
              max={20}
              step={1}
              value={draft.text_safe_zone_percent}
              onChange={(e) =>
                update({ text_safe_zone_percent: Number(e.target.value) })
              }
              className="w-full accent-[var(--color-accent)]"
            />
            {/* Visual safe-zone indicator */}
            <div
              className="relative mt-2 aspect-video w-full overflow-hidden rounded-md bg-[var(--color-bg-surface)] ring-1 ring-[var(--color-border)]"
              aria-hidden
            >
              <div
                className="absolute border-2 border-dashed border-[var(--color-accent)]/60"
                style={{
                  inset: `${draft.text_safe_zone_percent}%`,
                }}
              />
              <div className="absolute inset-0 flex items-center justify-center text-[10px] text-[var(--color-fg-muted)]">
                text area
              </div>
            </div>
          </div>

          {/* Transition */}
          <FieldRow label={t("setOutDispTransition")}>
            <div className="flex flex-wrap gap-1.5">
              {TRANSITIONS.map((tr) => (
                <button
                  key={tr.value}
                  type="button"
                  onClick={() => update({ transition: tr.value })}
                  className={cn(
                    "rounded-md border px-3 py-1 text-sm transition-colors",
                    draft.transition === tr.value
                      ? "border-[var(--color-accent)] bg-[var(--color-accent)]/15 text-[var(--color-fg)]"
                      : "border-[var(--color-border)] text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]",
                  )}
                >
                  {t(tr.labelKey as Parameters<typeof t>[0])}
                </button>
              ))}
            </div>
          </FieldRow>

          {/* Transition duration — only shown when not cut */}
          {showTransitionMs && (
            <div>
              <div className="mb-1 flex items-center justify-between text-sm">
                <span className="text-[var(--color-fg-muted)]">
                  {t("setOutDispTransitionMs")}
                </span>
                <span className="font-mono text-xs text-[var(--color-fg)]">
                  {draft.transition_ms}ms
                </span>
              </div>
              <p className="mb-1 text-xs text-[var(--color-fg-muted)]">
                {t("setOutDispTransitionMsDesc")}
              </p>
              <input
                type="range"
                min={0}
                max={1000}
                step={50}
                value={draft.transition_ms}
                onChange={(e) =>
                  update({ transition_ms: Number(e.target.value) })
                }
                className="w-full accent-[var(--color-accent)]"
              />
            </div>
          )}

          {/* Save feedback */}
          <div className="flex justify-end">
            {saved && (
              <span className="text-xs text-[var(--color-success)]">
                {t("setOutDispSaved")}
              </span>
            )}
          </div>
        </CardContent>
      </Card>
    </>
  );
}

function FieldRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="min-w-0">
        <div className="text-sm">{label}</div>
        {description && (
          <div className="text-xs text-[var(--color-fg-muted)]">
            {description}
          </div>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
