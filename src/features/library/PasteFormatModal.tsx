/**
 * PasteFormatModal — Phase 4.2 "Paste & Format with AI".
 *
 * Paste raw lyrics → format into structured sections + a proposed arrangement
 * → review → apply. Uses Claude when a key is available (a one-off pasted key,
 * or the key stored in the OS keychain via Settings) and falls back to the
 * local heuristic formatter otherwise. Cloud use is gated by a one-time consent
 * dialog. The backend always returns a usable draft with warnings, never fails.
 */

import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Sparkles, X } from "lucide-react";

import type { FormattedSong } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { cn } from "@/lib/cn";
import { hasAiConsent, grantAiConsent, preferredModel } from "@/lib/aiConsent";
import { ConsentDialog } from "@/components/ConsentDialog";
import { useT } from "@/lib/i18n";

function humanize(label: string): string {
  return label
    .split("_")
    .map((p) => (p ? p[0].toUpperCase() + p.slice(1) : ""))
    .join(" ");
}

interface PasteFormatModalProps {
  songId: string;
  onClose: () => void;
  onApplied: (arrangementId: string) => void;
}

export function PasteFormatModal({
  songId,
  onClose,
  onApplied,
}: PasteFormatModalProps) {
  const t = useT();
  const [raw, setRaw] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState(preferredModel() ?? "claude-sonnet-4-6");
  const [draft, setDraft] = useState<FormattedSong | null>(null);
  const [consentOpen, setConsentOpen] = useState(false);

  const modelsQuery = useQuery({
    queryKey: ["aiModels"],
    queryFn: () => ipc.ai.models(),
  });
  const keyStatusQuery = useQuery({
    queryKey: ["aiKeyStatus"],
    queryFn: () => ipc.ai.keyStatus(),
  });

  const formatMut = useMutation({
    mutationFn: () => ipc.ai.formatLyrics(raw, apiKey.trim() || null, model),
    onSuccess: (f) => setDraft(f),
  });

  // Will this run actually hit the cloud? (a pasted key, or a stored/env key)
  const willUseAi =
    apiKey.trim() !== "" ||
    !!keyStatusQuery.data?.stored ||
    !!keyStatusQuery.data?.env;

  function attemptFormat() {
    if (willUseAi && !hasAiConsent()) setConsentOpen(true);
    else formatMut.mutate();
  }

  const applyMut = useMutation({
    mutationFn: (f: FormattedSong) => ipc.ai.applyFormat(songId, f),
    onSuccess: (arr) => {
      onApplied(arr.id);
      onClose();
    },
  });

  return (
    <div className="fixed inset-0 z-50 grid place-items-center p-6">
      <div
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
        onClick={onClose}
        aria-hidden
      />
      <div className="relative flex max-h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <header className="flex items-center gap-2 border-b border-[var(--color-border)] px-4 py-3">
          <Sparkles size={16} className="text-[var(--color-accent)]" />
          <h2 className="font-semibold">{t("pasteTitle")}</h2>
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            className="grid h-7 w-7 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={15} />
          </button>
        </header>

        <div className="grid flex-1 grid-cols-2 gap-4 overflow-hidden p-4">
          {/* Input */}
          <div className="flex flex-col gap-3 overflow-hidden">
            <textarea
              value={raw}
              onChange={(e) => setRaw(e.target.value)}
              placeholder={t("pasteRawPlaceholder")}
              className="min-h-0 flex-1 resize-none rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] p-3 text-sm leading-snug focus:border-[var(--color-accent)] focus:outline-none"
            />
            <div className="space-y-2">
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t("pasteApiKeyOptional")}
                className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
              />
              <div className="flex items-center gap-2">
                <select
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  className="rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
                >
                  {(modelsQuery.data ?? []).map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.display}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  onClick={attemptFormat}
                  disabled={raw.trim().length === 0 || formatMut.isPending}
                  className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
                >
                  <Sparkles size={14} />
                  {formatMut.isPending
                    ? t("pasteFormatting")
                    : t("pasteFormat")}
                </button>
              </div>
              <p className="text-[10px] text-[var(--color-fg-muted)]">
                {keyStatusQuery.data?.stored
                  ? t("pasteUsingStoredKey")
                  : t("pasteNoKeyHint")}
              </p>
            </div>
          </div>

          {/* Preview */}
          <div className="flex flex-col overflow-hidden rounded-md border border-[var(--color-border)] bg-[var(--color-bg)]">
            {draft ? (
              <div className="flex-1 space-y-3 overflow-y-auto p-3">
                {draft.title_suggestion && (
                  <p className="text-sm">
                    <span className="text-[var(--color-fg-muted)]">
                      {t("pasteTitleSuggestion")}
                    </span>
                    {draft.title_suggestion}
                  </p>
                )}
                <div className="flex flex-wrap gap-1">
                  {draft.arrangement.map((label, i) => (
                    <span
                      key={`${label}-${i}`}
                      className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-[11px]"
                    >
                      {humanize(label)}
                    </span>
                  ))}
                </div>
                {draft.sections.map((s) => (
                  <div
                    key={s.label}
                    className="rounded-md border border-[var(--color-border)] p-2"
                  >
                    <div className="mb-1 text-[10px] font-semibold uppercase tracking-widest text-[var(--color-accent)]">
                      {humanize(s.label)}
                    </div>
                    <pre className="whitespace-pre-wrap font-sans text-xs text-[var(--color-fg-muted)]">
                      {s.lyrics}
                    </pre>
                  </div>
                ))}
                {draft.warnings.length > 0 && (
                  <ul className="space-y-1 border-t border-[var(--color-border)] pt-2">
                    {draft.warnings.map((w, i) => (
                      <li
                        key={i}
                        className="text-[11px] text-[var(--color-warning)]"
                      >
                        ⚠ {w}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            ) : (
              <div className="grid flex-1 place-items-center p-4 text-center text-sm text-[var(--color-fg-muted)]">
                {t("pasteResultHint")}
              </div>
            )}
          </div>
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-[var(--color-border)] px-4 py-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            {t("actionCancel")}
          </button>
          <button
            type="button"
            onClick={() => draft && applyMut.mutate(draft)}
            disabled={!draft || applyMut.isPending}
            className={cn(
              "rounded-md bg-[var(--color-accent)] px-4 py-1.5 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-40",
            )}
          >
            {applyMut.isPending ? t("pasteApplying") : t("pasteApply")}
          </button>
        </footer>
      </div>

      <ConsentDialog
        open={consentOpen}
        onClose={() => setConsentOpen(false)}
        onAccept={() => {
          grantAiConsent();
          setConsentOpen(false);
          formatMut.mutate();
        }}
      />
    </div>
  );
}
