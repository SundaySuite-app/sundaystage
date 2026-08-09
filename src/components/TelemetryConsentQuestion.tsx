/**
 * E6 — the consent question itself, in one place.
 *
 * Two surfaces ask it: the onboarding step (a fresh install, mid-wizard) and the
 * corner card (an install that finished onboarding long before this feature
 * existed). They differ in chrome and in nothing else, which is the point of
 * this component: the words an operator agrees to must not be able to drift
 * apart between the two places they are asked.
 *
 * Three rules the markup encodes, from the programme's locked owner decisions:
 *
 *   1. **The two answers are equally weighted.** Same size, same shape, same
 *      row — "no" is not a grey whisper next to a bright "yes". A consent dialog
 *      that visually pushes one answer is not asking, it is nudging.
 *   2. **"What is sent" is available before answering**, as an expander rather
 *      than a link away — the three categories are the scope, and the scope is
 *      what the question is about.
 *   3. **The "never" line is always visible**, not folded into the expander. It
 *      is the promise the payload builder and the endpoint's validator actually
 *      enforce, so it is the sentence most worth reading.
 */
import { useState } from "react";
import { ChevronDown, ChevronRight, ExternalLink } from "lucide-react";

import { openUrl } from "@tauri-apps/plugin-opener";

import { cn } from "@/lib/cn";
import { useT } from "@/lib/i18n";

/**
 * Where "read more" goes.
 *
 * The repository copy is the canonical text and ships with the source, so the
 * link cannot point at a page that says something the running build does not do.
 * 👤 Repoint this at sundaysuite.app once the privacy page is published there.
 */
export const PRIVACY_URL =
  "https://github.com/SundaySuite-app/sundaystage/blob/main/PRIVACY.md";

interface TelemetryConsentQuestionProps {
  /** Called with the operator's answer. Both buttons are answers. */
  onAnswer: (granted: boolean) => void;
  /** True while the answer is being written, so the buttons cannot double-fire. */
  pending?: boolean;
  /** Tighter type for the corner card; the onboarding step uses the roomy one. */
  compact?: boolean;
  className?: string;
}

export function TelemetryConsentQuestion({
  onAnswer,
  pending = false,
  compact = false,
  className,
}: TelemetryConsentQuestionProps) {
  const t = useT();
  const [showDetail, setShowDetail] = useState(false);

  return (
    <div className={cn("text-left", className)}>
      <h2
        className={cn(
          "font-semibold",
          compact ? "text-[var(--text-ui-md)]" : "text-[var(--text-ui-xl)]",
        )}
      >
        {t("telConsentTitle")}
      </h2>
      <p
        className={cn(
          "mt-2 leading-relaxed text-[var(--color-fg-muted)]",
          compact ? "text-xs" : "text-sm",
        )}
      >
        {t("telConsentBody")}
      </p>

      <button
        type="button"
        aria-expanded={showDetail}
        onClick={() => setShowDetail((v) => !v)}
        className="mt-3 flex items-center gap-1 text-xs font-medium text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]"
      >
        {showDetail ? (
          <ChevronDown size={14} aria-hidden />
        ) : (
          <ChevronRight size={14} aria-hidden />
        )}
        {t("telConsentWhatIsSent")}
      </button>
      {showDetail && (
        <ul className="mt-2 space-y-1.5 rounded-lg bg-[var(--color-bg-surface)] p-3 text-xs text-[var(--color-fg-muted)]">
          <li>{t("telConsentCatCrashes")}</li>
          <li>{t("telConsentCatQuality")}</li>
          <li>{t("telConsentCatUsage")}</li>
        </ul>
      )}

      <p className="mt-3 text-xs leading-relaxed text-[var(--color-fg-muted)]">
        {t("telConsentNever")}
      </p>

      <button
        type="button"
        onClick={() => {
          // Fire-and-forget: a browser without a Tauri host (the e2e harness)
          // must not turn "read more" into an unhandled rejection.
          void openUrl(PRIVACY_URL).catch(() => {
            /* no opener available — the text above still stands on its own */
          });
        }}
        className="mt-2 flex items-center gap-1 text-xs text-[var(--color-accent)] hover:underline"
      >
        {t("telConsentPrivacyLink")}
        <ExternalLink size={12} aria-hidden />
      </button>

      {/* Equally weighted, deliberately: same variant, same size, same row. */}
      <div className="mt-5 flex flex-wrap gap-2">
        <button
          type="button"
          disabled={pending}
          onClick={() => onAnswer(true)}
          className="flex-1 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-4 py-2 text-sm font-medium text-[var(--color-fg)] transition-colors hover:border-[var(--color-accent)] disabled:opacity-50"
        >
          {t("telConsentYes")}
        </button>
        <button
          type="button"
          disabled={pending}
          onClick={() => onAnswer(false)}
          className="flex-1 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-4 py-2 text-sm font-medium text-[var(--color-fg)] transition-colors hover:border-[var(--color-accent)] disabled:opacity-50"
        >
          {t("telConsentNo")}
        </button>
      </div>
    </div>
  );
}
