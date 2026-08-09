/**
 * E6 — "Report a problem".
 *
 * The one place in SundayStage where an operator's own words can leave the
 * machine, so the dialog is built around showing them exactly what leaves:
 *
 *   * the **preview is the outgoing bytes**, not a rendering of them. It comes
 *     from `telemetry_report_preview`, which builds the report through the same
 *     `ProblemReport::new` the submit path uses — same scrub, same caps. If a
 *     typed message contains a path, the preview shows it replaced with
 *     `<path>`, because that is what would be sent;
 *   * the **log tail shown is the log tail sent**. `submit` takes the previewed
 *     text back rather than re-reading the file, so lines written between
 *     preview and send cannot ride along unseen;
 *   * **sending works with anonymous sharing off**. Pressing send is consent for
 *     this one report: it travels alone under a one-shot id that is stored
 *     nowhere, and no durable install id is created. The note above the button
 *     says so before it is pressed.
 *
 * The five outcomes are reported as they happened — including "this build has no
 * recipient compiled in", which is every development build and every build until
 * the release that bakes the endpoint in. A dialog that said "sent!" there would
 * be the most damaging lie in the feature.
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, Send } from "lucide-react";

import type { ReportContext, ReportOutcome } from "@/lib/bindings";
import { Button, Dialog, Select, Textarea } from "@/components/ui";
import { ipc } from "@/lib/ipc";
import { useT, type TKey } from "@/lib/i18n";

/** Mirrors `telemetry::MESSAGE_MAX_CHARS`. */
export const REPORT_MESSAGE_MAX = 200;

/** How long after the last keystroke the preview is rebuilt. */
const PREVIEW_DEBOUNCE_MS = 300;

const CONTEXTS: Array<{ value: ReportContext; labelKey: TKey }> = [
  { value: "live", labelKey: "reportContextLive" },
  { value: "editor", labelKey: "reportContextEditor" },
  { value: "settings", labelKey: "reportContextSettings" },
  { value: "other", labelKey: "reportContextOther" },
];

const OUTCOME_KEY: Record<ReportOutcome, TKey> = {
  queued: "reportOutcomeQueued",
  sent: "reportOutcomeSent",
  "deferred-live": "reportOutcomeDeferredLive",
  "deferred-offline": "reportOutcomeDeferredOffline",
  "no-endpoint": "reportOutcomeNoEndpoint",
};

interface ProblemReportDialogProps {
  open: boolean;
  onClose: () => void;
  /** Where the operator was when they opened it — a default, never a lock. */
  defaultContext?: ReportContext;
}

export function ProblemReportDialog({
  open,
  onClose,
  defaultContext = "other",
}: ProblemReportDialogProps) {
  const t = useT();
  const qc = useQueryClient();
  const [context, setContext] = useState<ReportContext>(defaultContext);
  const [message, setMessage] = useState("");
  const [debounced, setDebounced] = useState("");
  const [outcome, setOutcome] = useState<ReportOutcome | null>(null);

  // Reset on every open: a dialog that reopened holding last week's text (or
  // last week's "thank you") would be reporting the wrong thing.
  useEffect(() => {
    if (!open) return;
    setContext(defaultContext);
    setMessage("");
    setDebounced("");
    setOutcome(null);
  }, [open, defaultContext]);

  useEffect(() => {
    const id = setTimeout(() => setDebounced(message), PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [message]);

  const consent = useQuery({
    queryKey: ["telemetryConsent"],
    queryFn: () => ipc.telemetry.consent.get(),
    enabled: open,
    retry: false,
  });

  const preview = useQuery({
    queryKey: ["telemetryReportPreview", context, debounced],
    queryFn: () => ipc.telemetry.report.preview(context, debounced),
    enabled: open,
    retry: false,
  });

  const submit = useMutation({
    mutationFn: () =>
      ipc.telemetry.report.submit(
        context,
        message,
        // Exactly what the preview showed — see the module docs.
        preview.data?.logTail ?? "",
      ),
    onSuccess: (result) => {
      setOutcome(result);
      void qc.invalidateQueries({ queryKey: ["telemetryQueue"] });
      void qc.invalidateQueries({ queryKey: ["telemetryPreview"] });
    },
  });

  const canSend =
    message.trim().length > 0 && !submit.isPending && preview.isSuccess;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("reportTitle")}
      description={t("reportDesc")}
      className="max-w-xl"
    >
      {outcome ? (
        <div className="space-y-4">
          <p className="text-sm text-[var(--color-fg)]">
            {t(OUTCOME_KEY[outcome])}
          </p>
          <div className="flex justify-end">
            <Button onClick={onClose}>{t("actionClose")}</Button>
          </div>
        </div>
      ) : (
        <div className="space-y-4">
          <div>
            <label
              htmlFor="report-message"
              className="mb-1 block text-xs text-[var(--color-fg-muted)]"
            >
              {t("reportMessageLabel")}
            </label>
            <Textarea
              id="report-message"
              autoFocus
              maxLength={REPORT_MESSAGE_MAX}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder={t("reportMessagePlaceholder")}
            />
            <div className="mt-1 text-right font-mono text-[11px] text-[var(--color-fg-muted)]">
              {t("reportCounter", {
                n: message.length,
                max: REPORT_MESSAGE_MAX,
              })}
            </div>
          </div>

          <div>
            <label
              htmlFor="report-context"
              className="mb-1 block text-xs text-[var(--color-fg-muted)]"
            >
              {t("reportContextLabel")}
            </label>
            <Select
              id="report-context"
              className="max-w-xs"
              value={context}
              onChange={(e) => setContext(e.target.value as ReportContext)}
            >
              {CONTEXTS.map((c) => (
                <option key={c.value} value={c.value}>
                  {t(c.labelKey)}
                </option>
              ))}
            </Select>
          </div>

          <div>
            <div className="mb-1 text-xs text-[var(--color-fg-muted)]">
              {t("reportLogLabel")}
            </div>
            <pre
              data-testid="report-log-preview"
              className="max-h-48 overflow-auto rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-[var(--color-fg-muted)]"
            >
              {preview.data?.logTail?.length
                ? preview.data.logTail
                : t("reportLogEmpty")}
            </pre>
          </div>

          {consent.data && !consent.data.active && (
            <p className="rounded-md bg-[var(--color-bg-surface)] p-3 text-xs leading-relaxed text-[var(--color-fg-muted)]">
              {t("reportEphemeralNote")}
            </p>
          )}

          {submit.isError && (
            <p className="text-xs text-[var(--color-danger)]">
              {t("reportFailed")}
            </p>
          )}

          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={onClose}>
              {t("actionCancel")}
            </Button>
            <Button disabled={!canSend} onClick={() => submit.mutate()}>
              {submit.isPending ? (
                <Loader2 size={14} className="animate-spin" />
              ) : (
                <Send size={14} />
              )}
              {submit.isPending ? t("reportSending") : t("reportSend")}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}
