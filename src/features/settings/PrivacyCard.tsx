/**
 * E6 — the «Personvern» card. Replaces the Advanced tab's crash-reporting card.
 *
 * Everything an operator can do about telemetry, in one place, with nothing
 * hidden behind a support email:
 *
 *   * **the switch** — on with one click, off with one click. No confirmation on
 *     enable (asking twice is not consent, it is pressure) and no "are you sure
 *     you want to make SundayStage worse?" on disable. Turning it off purges the
 *     outbox and the accumulated counters in Rust, so off means there is nothing
 *     left to send rather than a paused pile;
 *   * **"show what is sent"** — the REAL payload builder's bytes, pretty-printed.
 *     Not a sample, not a description: `telemetry_preview_payload` runs the same
 *     code the wire uses, and a test pins the two byte-for-byte. With sharing off
 *     there is no "next payload", so it honestly shows everything the machine
 *     holds instead;
 *   * **the queue** — how many payloads wait, how old the oldest is, and
 *     separately whether a hand-written problem report is still owed;
 *   * **"delete my data"** — works whether sharing is on or off, because the
 *     person most entitled to it is the one who already said no. It is the one
 *     button here with a confirmation, and only because it is irreversible and
 *     remote;
 *   * **the install id** — the whole of what identifies this machine, shown in
 *     full, with a regenerate button that retires it.
 *
 * The always-on local crash records keep their count and clear button in the
 * footer: they never leave the machine, so they need no permission — but hiding
 * them entirely would make the card less honest, not more.
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fingerprint, MessageSquareWarning, ShieldCheck } from "lucide-react";

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  ConfirmModal,
} from "@/components/ui";
import { ipc } from "@/lib/ipc";
import { useLocale, useT } from "@/lib/i18n";
import { ProblemReportDialog } from "./ProblemReportDialog";
import { ToggleRow } from "./ToggleRow";

export function PrivacyCard() {
  const t = useT();
  const lang = useLocale((s) => s.lang);
  const qc = useQueryClient();
  const [showPayload, setShowPayload] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleted, setDeleted] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);

  const consent = useQuery({
    queryKey: ["telemetryConsent"],
    queryFn: () => ipc.telemetry.consent.get(),
    retry: false,
  });
  const queue = useQuery({
    queryKey: ["telemetryQueue"],
    queryFn: () => ipc.telemetry.queueStatus(),
    retry: false,
  });
  const installId = useQuery({
    queryKey: ["telemetryInstallId"],
    queryFn: () => ipc.telemetry.installId(),
    retry: false,
  });
  const preview = useQuery({
    queryKey: ["telemetryPreview"],
    queryFn: () => ipc.telemetry.previewPayload(),
    enabled: showPayload,
    retry: false,
  });
  const crashCount = useQuery({
    queryKey: ["crashCount"],
    queryFn: () => ipc.crash.count(),
    retry: false,
  });

  const refreshAll = () => {
    void qc.invalidateQueries({ queryKey: ["telemetryQueue"] });
    void qc.invalidateQueries({ queryKey: ["telemetryInstallId"] });
    void qc.invalidateQueries({ queryKey: ["telemetryPreview"] });
  };

  const setConsent = useMutation({
    mutationFn: (granted: boolean) => ipc.telemetry.consent.set(granted),
    onSuccess: (next) => {
      qc.setQueryData(["telemetryConsent"], next);
      refreshAll();
    },
  });
  const regenerate = useMutation({
    mutationFn: () => ipc.telemetry.regenerateInstallId(),
    onSuccess: refreshAll,
  });
  const deleteData = useMutation({
    mutationFn: () => ipc.telemetry.deleteMyData(),
    onSuccess: () => {
      setDeleted(true);
      refreshAll();
      void qc.invalidateQueries({ queryKey: ["crashCount"] });
    },
  });
  const clearCrashes = useMutation({
    mutationFn: () => ipc.crash.clear(),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["crashCount"] }),
  });
  const nativeCrash = useQuery({
    queryKey: ["nativeCrashStatus"],
    queryFn: () => ipc.crash.native.status(),
    retry: false,
  });
  const setNativeCrash = useMutation({
    mutationFn: (enabled: boolean) => ipc.crash.native.set(enabled),
    onSuccess: (next) => qc.setQueryData(["nativeCrashStatus"], next),
  });

  const status = consent.data;
  const active = status?.active ?? false;
  const stateLabel = !status
    ? t("setPrivacyStateOff")
    : status.status === "never-asked" || status.needsPrompt
      ? t("setPrivacyStateNotAsked")
      : active
        ? t("setPrivacyStateOn")
        : t("setPrivacyStateOff");

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <ShieldCheck size={18} className="text-[var(--color-accent)]" />
            {t("setPrivacyTitle")}
          </CardTitle>
          <CardDescription>{t("setPrivacyDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-[var(--color-fg-muted)]">
              {t("setStatus")}
            </span>
            <Badge variant={active ? "success" : "neutral"}>{stateLabel}</Badge>
          </div>

          {/* One click on, one click off. No confirmation either way. */}
          <ToggleRow
            label={t("setPrivacyShareLabel")}
            description={t("setPrivacyShareDesc")}
            checked={active}
            disabled={setConsent.isPending || consent.isLoading}
            onChange={(v) => setConsent.mutate(v)}
          />

          {/* ── What would be sent, from the real builder ─────────────── */}
          <div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowPayload((v) => !v)}
            >
              {showPayload
                ? t("setPrivacyHidePayload")
                : t("setPrivacyShowPayload")}
            </Button>
            {showPayload && (
              <div className="mt-3 space-y-2">
                <p className="text-xs text-[var(--color-fg-muted)]">
                  {preview.data?.isNextPayload
                    ? t("setPrivacyPayloadNext")
                    : t("setPrivacyPayloadWouldSend")}
                </p>
                {preview.data?.isEmpty && (
                  <p className="text-xs text-[var(--color-fg-muted)]">
                    {t("setPrivacyPayloadEmpty")}
                  </p>
                )}
                <pre
                  data-testid="telemetry-payload-preview"
                  className="max-h-72 overflow-auto rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] p-3 font-mono text-[11px] leading-relaxed text-[var(--color-fg-muted)]"
                >
                  {preview.data?.json ?? ""}
                </pre>
              </div>
            )}
          </div>

          {/* ── The queue, said plainly ───────────────────────────────── */}
          <div className="space-y-1 text-xs text-[var(--color-fg-muted)]">
            <p>
              {queue.data && queue.data.pending > 0
                ? t("setPrivacyQueue", {
                    n: queue.data.pending,
                    date: formatWhen(queue.data.oldestAt, lang),
                  })
                : t("setPrivacyQueueEmpty")}
            </p>
            {/* Its own line, because a deferred report is owed even when every
                queued payload went out cleanly — the byte trim must never be
                where an operator's words quietly disappear. */}
            {queue.data && queue.data.pendingReports > 0 && (
              <p data-testid="telemetry-reports-waiting">
                {t("setPrivacyReportsWaiting", {
                  n: queue.data.pendingReports,
                })}
              </p>
            )}
            {queue.data && queue.data.failed > 0 && (
              <p>{t("setPrivacyQueueFailed", { n: queue.data.failed })}</p>
            )}
            {queue.data?.lastError && (
              <p>{t("setPrivacyLastError", { error: queue.data.lastError })}</p>
            )}
          </div>

          {/* ── Identity ──────────────────────────────────────────────── */}
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-[var(--color-bg-surface)] p-3">
            <div className="min-w-0">
              <div className="flex items-center gap-1.5 text-xs text-[var(--color-fg-muted)]">
                <Fingerprint size={13} aria-hidden />
                {t("setPrivacyInstallId")}
              </div>
              <div className="mt-1 font-mono text-[11px] break-all text-[var(--color-fg)]">
                {installId.data ?? t("setPrivacyInstallIdNone")}
              </div>
              <div className="mt-1 text-[11px] text-[var(--color-fg-muted)]">
                {t("setPrivacyRegenerateDesc")}
              </div>
            </div>
            <Button
              variant="outline"
              size="sm"
              disabled={regenerate.isPending || !installId.data}
              onClick={() => regenerate.mutate()}
            >
              {t("setPrivacyRegenerate")}
            </Button>
          </div>

          {/* ── Deletion — never consent-gated ────────────────────────── */}
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="max-w-md">
              <div className="text-sm">{t("setPrivacyDelete")}</div>
              <div className="text-xs text-[var(--color-fg-muted)]">
                {t("setPrivacyDeleteDesc")}
              </div>
              {deleted && (
                <div className="mt-1 text-xs text-[var(--color-success)]">
                  {t("setPrivacyDeleteDone")}
                </div>
              )}
            </div>
            <Button
              variant="outline"
              size="sm"
              disabled={deleteData.isPending}
              onClick={() => setConfirmDelete(true)}
            >
              {t("setPrivacyDelete")}
            </Button>
          </div>

          <div>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setReportOpen(true)}
            >
              <MessageSquareWarning size={14} />
              {t("setPrivacyReportProblem")}
            </Button>
          </div>
        </CardContent>

        {/* ── Always-on local capture ───────────────────────────────────── */}
        <CardFooter className="flex flex-col gap-4 border-t border-[var(--color-border)] pt-4">
          <div className="flex w-full flex-wrap items-center justify-between gap-3">
            <div className="text-xs text-[var(--color-fg-muted)]">
              <div className="text-[var(--color-fg)]">
                {t("setPrivacyLocalCrashes")}
              </div>
              <div>{t("setPrivacyLocalCrashesDesc")}</div>
              <div className="mt-1">
                {t("setCrashCount", { n: crashCount.data ?? 0 })}
              </div>
            </div>
            {(crashCount.data ?? 0) > 0 && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => clearCrashes.mutate()}
              >
                {t("actionClear")}
              </Button>
            )}
          </div>

          {/* A6 — the one capture switch there IS, because installing a signal
              handler changes how the process behaves while it is dying. What it
              captures is a signal type and an offset; never a memory dump. */}
          <div className="w-full">
            <ToggleRow
              label={t("setPrivacyHardCrashes")}
              description={t("setPrivacyHardCrashesDesc")}
              checked={nativeCrash.data?.enabled ?? true}
              onChange={(v) => setNativeCrash.mutate(v)}
              disabled={setNativeCrash.isPending}
            />
            {nativeCrash.data?.enabled && !nativeCrash.data.armed && (
              <div className="mt-1 text-xs text-[var(--color-warning)]">
                {t("setPrivacyHardCrashesUnavailable")}
              </div>
            )}
          </div>
        </CardFooter>
      </Card>

      {confirmDelete && (
        <ConfirmModal
          title={t("setPrivacyDeleteTitle")}
          body={t("setPrivacyDeleteBody")}
          confirmLabel={t("setPrivacyDelete")}
          cancelLabel={t("actionCancel")}
          onCancel={() => setConfirmDelete(false)}
          onConfirm={() => {
            setConfirmDelete(false);
            deleteData.mutate();
          }}
        />
      )}

      <ProblemReportDialog
        open={reportOpen}
        onClose={() => setReportOpen(false)}
        defaultContext="settings"
      />
    </>
  );
}

/** The oldest queued payload's timestamp, in the operator's own locale. */
function formatWhen(at: number | null, lang: string): string {
  if (!at) return "—";
  try {
    return new Date(at).toLocaleString(lang);
  } catch {
    return new Date(at).toISOString();
  }
}
