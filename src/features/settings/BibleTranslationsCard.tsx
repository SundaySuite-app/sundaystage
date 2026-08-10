/**
 * Spor C (C1/C2) — download full Bible corpora.
 *
 * The app ships a small curated starter set (the passages churches actually
 * project) so a fresh offline install already has scripture. The complete
 * public-domain Bibles (~8 MB each) are fetched on demand from a pinned,
 * checksum-verified source and installed into the same tables — this card is
 * where the operator picks and downloads them.
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { BookOpen, Check, Download, Loader2 } from "lucide-react";

import { ipc, BIBLE_DOWNLOAD_PROGRESS_EVENT } from "@/lib/ipc";
import type {
  AvailableTranslation,
  BibleDownloadProgress,
} from "@/lib/bindings";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui";
import { langLabel, useT, type TKey } from "@/lib/i18n";

/** A code whose installed verse count is this high or more is the full corpus,
 *  not the bundled starter set (starter sets are a few dozen verses at most). */
const FULL_CORPUS_MIN_VERSES = 1000;

const PHASE_KEY: Record<BibleDownloadProgress["phase"], TKey> = {
  downloading: "bibleDlDownloading",
  verifying: "bibleDlVerifying",
  installing: "bibleDlInstalling",
  done: "bibleDlInstalling",
};

function mb(bytes: number): string {
  return (bytes / 1_000_000).toFixed(0);
}

function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}

export function BibleTranslationsCard() {
  const t = useT();
  const qc = useQueryClient();
  const available = useQuery({
    queryKey: ["bibleAvailable"],
    queryFn: () => ipc.bible.availableTranslations(),
  });

  // Live progress, keyed by translation code, fed by the Rust event stream.
  const [progress, setProgress] = useState<
    Record<string, BibleDownloadProgress | undefined>
  >({});
  const [errors, setErrors] = useState<Record<string, string | undefined>>({});

  useEffect(() => {
    const unlisten = listen<BibleDownloadProgress>(
      BIBLE_DOWNLOAD_PROGRESS_EVENT,
      (event) => {
        const p = event.payload;
        setProgress((prev) => ({ ...prev, [p.code]: p }));
      },
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const download = useMutation({
    mutationFn: (code: string) => ipc.bible.download(code),
    onMutate: (code) => {
      setErrors((prev) => ({ ...prev, [code]: undefined }));
    },
    onError: (err, code) => {
      setErrors((prev) => ({ ...prev, [code]: errorMessage(err) }));
    },
    onSettled: (_data, _err, code) => {
      setProgress((prev) => ({ ...prev, [code]: undefined }));
      void qc.invalidateQueries({ queryKey: ["bibleAvailable"] });
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <BookOpen className="size-4" />
          {t("bibleDlTitle")}
        </CardTitle>
        <CardDescription>{t("bibleDlDesc")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {available.data?.map((tr) => (
          <TranslationRow
            key={tr.code}
            translation={tr}
            progress={progress[tr.code]}
            error={errors[tr.code]}
            downloading={download.isPending && download.variables === tr.code}
            onDownload={() => download.mutate(tr.code)}
          />
        ))}
      </CardContent>
    </Card>
  );
}

function TranslationRow({
  translation,
  progress,
  error,
  downloading,
  onDownload,
}: {
  translation: AvailableTranslation;
  progress: BibleDownloadProgress | undefined;
  error: string | undefined;
  downloading: boolean;
  onDownload: () => void;
}) {
  const t = useT();
  const isFull = translation.installedVerses >= FULL_CORPUS_MIN_VERSES;
  const busy = downloading || progress !== undefined;

  const pct =
    progress && progress.phase === "downloading" && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : progress
        ? 100
        : 0;

  return (
    <div className="rounded-md border border-[var(--color-border)] p-3">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-medium">{translation.name}</span>
            <Badge variant="neutral">{langLabel(translation.language)}</Badge>
            {isFull && (
              <Badge variant="success">
                <Check className="size-3" /> {t("bibleDlInstalled")}
              </Badge>
            )}
          </div>
          <p className="mt-0.5 text-xs text-[var(--color-fg-muted)]">
            {t("bibleDlSizeMb", { mb: mb(translation.approxBytes) })}
          </p>
        </div>
        <Button
          variant={isFull ? "outline" : "primary"}
          size="sm"
          disabled={busy}
          onClick={onDownload}
        >
          {busy ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <Download className="size-4" />
          )}
          {isFull ? t("bibleDlRedownload") : t("bibleDlDownload")}
        </Button>
      </div>

      {progress && (
        <div className="mt-3">
          <div className="h-1.5 overflow-hidden rounded-full bg-[var(--color-border)]">
            <div
              className="h-full rounded-full bg-[var(--color-accent)] transition-all"
              style={{ width: `${pct}%` }}
            />
          </div>
          <p className="mt-1 text-xs text-[var(--color-fg-muted)]">
            {t(PHASE_KEY[progress.phase])}
          </p>
        </div>
      )}

      {error && (
        <p className="mt-2 text-xs text-[var(--color-danger)]">
          {t("bibleDlFailed", { error })}
        </p>
      )}
    </div>
  );
}
