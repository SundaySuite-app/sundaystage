/**
 * A7 — «Sangbruk», rapportkortet.
 *
 * Menigheten må fortelle TONO — og CCLI for det som er lisensiert der — hvilke
 * sanger den faktisk har brukt. Kortet gjør tre ting og ikke mer:
 *
 *   * **velg periode** — tre hurtigvalg for de tre måtene en menighet faktisk
 *     rapporterer på (året, i fjor, kvartalet), pluss to datofelt for alt
 *     annet;
 *   * **lag fila** — én CSV med de kolonnene skjemaene spør etter. Den skrives
 *     i appens egen rapportmappe og eier får knappen som åpner mappa. Hva som
 *     skjer med fila etterpå er eiers sak: ingenting her sender noe;
 *   * **slett loggen** — ett trykk, én bekreftelse fordi det ikke kan angres.
 *
 * Listen viser hva perioden faktisk inneholder før eier lager fila, med
 * «mangler»-merket synlig per sang. En rapport man skal signere tåler ikke at
 * tomme felt ser komplette ut.
 *
 * Kortet ligger under Avansert, ved siden av personvernkortet: loggen er
 * menighetens innhold, og sletteknappen hører hjemme der de andre svarene på
 * «hva ligger igjen på denne maskinen» står.
 */
import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FileSpreadsheet, FolderOpen, ListMusic } from "lucide-react";

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
  Input,
} from "@/components/ui";
import { ipc } from "@/lib/ipc";
import { useT, type TKey } from "@/lib/i18n";
import {
  PERIOD_PRESETS,
  periodBounds,
  presetPeriod,
  type Period,
  type PeriodPreset,
} from "./songUsagePeriod";

const PRESET_LABEL: Record<PeriodPreset, TKey> = {
  thisYear: "suPresetThisYear",
  lastYear: "suPresetLastYear",
  lastQuarter: "suPresetLastQuarter",
};

export function SongUsageCard() {
  const t = useT();
  const qc = useQueryClient();
  const [period, setPeriod] = useState<Period>(() =>
    presetPeriod("thisYear", new Date()),
  );
  const [confirmClear, setConfirmClear] = useState(false);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  const bounds = useMemo(() => periodBounds(period), [period]);

  const rows = useQuery({
    queryKey: ["songUsage", bounds.fromMs, bounds.toMs],
    queryFn: () => ipc.songUsage.list(bounds.fromMs, bounds.toMs),
  });
  const total = useQuery({
    queryKey: ["songUsageCount"],
    queryFn: () => ipc.songUsage.count(),
  });

  const exportCsv = useMutation({
    mutationFn: () => ipc.songUsage.exportCsv(bounds.fromMs, bounds.toMs),
    onSuccess: (path) => {
      setSavedPath(path);
      setFailed(false);
    },
    onError: () => {
      setSavedPath(null);
      setFailed(true);
    },
  });
  const clear = useMutation({
    mutationFn: () => ipc.songUsage.clear(),
    onSuccess: () => {
      setSavedPath(null);
      void qc.invalidateQueries({ queryKey: ["songUsage"] });
      void qc.invalidateQueries({ queryKey: ["songUsageCount"] });
    },
  });

  const list = rows.data ?? [];
  const uses = list.reduce((n, r) => n + Math.max(1, Number(r.show_count)), 0);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ListMusic size={16} className="text-[var(--color-accent)]" />
          {t("suTitle")}
        </CardTitle>
        <CardDescription>{t("suDesc")}</CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-1.5">
          {PERIOD_PRESETS.map((preset) => (
            <Button
              key={preset}
              variant="outline"
              onClick={() => setPeriod(presetPeriod(preset, new Date()))}
            >
              {t(PRESET_LABEL[preset])}
            </Button>
          ))}
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-xs text-[var(--color-fg-muted)]">
            {t("suFrom")}
            <Input
              type="date"
              value={period.from}
              onChange={(e) => setPeriod({ ...period, from: e.target.value })}
            />
          </label>
          <label className="flex flex-col gap-1 text-xs text-[var(--color-fg-muted)]">
            {t("suTo")}
            <Input
              type="date"
              value={period.to}
              onChange={(e) => setPeriod({ ...period, to: e.target.value })}
            />
          </label>
        </div>

        <p className="text-sm text-[var(--color-fg-muted)]">
          {rows.isLoading
            ? t("suLoading")
            : t("suSummary", { songs: list.length, uses })}
        </p>

        {list.length > 0 && (
          <ul className="max-h-56 space-y-1 overflow-y-auto">
            {list.map((row) => (
              <li
                key={row.id}
                className="flex items-center gap-3 rounded-md border border-[var(--color-border)] px-3 py-1.5 text-sm"
              >
                <span className="font-mono text-xs tabular-nums text-[var(--color-fg-muted)]">
                  {row.service_date}
                </span>
                <span className="truncate">{row.title}</span>
                <div className="flex-1" />
                {Number(row.show_count) > 1 && (
                  <Badge variant="neutral">
                    {t("suTimes", { n: Number(row.show_count) })}
                  </Badge>
                )}
                {row.ccli_song_id ? (
                  <Badge variant="neutral">CCLI {row.ccli_song_id}</Badge>
                ) : (
                  <Badge variant="warning">{t("suMissingCcli")}</Badge>
                )}
              </li>
            ))}
          </ul>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <Button
            onClick={() => exportCsv.mutate()}
            disabled={exportCsv.isPending || list.length === 0}
          >
            <FileSpreadsheet size={14} />
            {exportCsv.isPending ? t("suExporting") : t("suExport")}
          </Button>
          {savedPath && (
            <Button
              variant="outline"
              onClick={() => void ipc.songUsage.openFolder()}
            >
              <FolderOpen size={14} />
              {t("suOpenFolder")}
            </Button>
          )}
        </div>

        {savedPath && (
          <p className="break-all text-xs text-[var(--color-fg-muted)]">
            {t("suSavedAt")} <span className="font-mono">{savedPath}</span>
          </p>
        )}
        {failed && (
          <p className="text-xs text-[var(--color-danger)]">{t("suFailed")}</p>
        )}
      </CardContent>

      <CardFooter className="flex flex-wrap items-center gap-3">
        <span className="text-xs text-[var(--color-fg-muted)]">
          {t("suRetention", { rows: total.data ?? 0 })}
        </span>
        <div className="flex-1" />
        <Button
          variant="outline"
          onClick={() => setConfirmClear(true)}
          disabled={(total.data ?? 0) === 0 || clear.isPending}
        >
          {t("suClear")}
        </Button>
      </CardFooter>

      {confirmClear && (
        <ConfirmModal
          title={t("suClearTitle")}
          body={t("suClearBody")}
          confirmLabel={t("suClear")}
          cancelLabel={t("actionCancel")}
          onConfirm={() => {
            setConfirmClear(false);
            clear.mutate();
          }}
          onCancel={() => setConfirmClear(false)}
        />
      )}
    </Card>
  );
}
