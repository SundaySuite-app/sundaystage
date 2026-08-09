/**
 * First-run welcome — Phase 13.1, with the E6 consent step.
 *
 * Three steps: pick a language → answer the telemetry question → seed a demo
 * "Velkomstgudstjeneste" or start empty. Shown once (gated by localStorage in
 * App) when a fresh library has no content yet.
 *
 * ## Why a STEPS array and not a wizard framework
 *
 * The same shape `TutorialOverlay` already uses: a literal array of steps, an
 * index in `useState`, and the panel renders `STEPS[step]`. Three steps do not
 * need a router, a state machine or a context — and the tutorial's version has
 * survived every layout change since Phase 13 precisely because there is nothing
 * to it.
 *
 * ## Skipping is not answering
 *
 * "Start empty" from step 1 leaves the consent record ABSENT, not denied — the
 * operator was never asked, so nothing may be sent and the corner card
 * (`TelemetryConsentCard`) will ask later. Only the two buttons on the consent
 * step write a record, and both of them do.
 */

import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Play, Sparkles } from "lucide-react";

import { TelemetryConsentQuestion } from "@/components/TelemetryConsentQuestion";
import type { Library } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { LANGS, langLabel, useLocale, useT } from "@/lib/i18n";
import { cn } from "@/lib/cn";

/** The steps, in order. Adding one is adding a line here and a branch below. */
const STEPS = ["language", "consent", "finish"] as const;
type Step = (typeof STEPS)[number];

interface WelcomeScreenProps {
  library: Library;
  onDone: () => void;
}

export function WelcomeScreen({ library, onDone }: WelcomeScreenProps) {
  const t = useT();
  const lang = useLocale((s) => s.lang);
  const setLang = useLocale((s) => s.setLang);
  const [step, setStep] = useState(0);
  const current: Step = STEPS[step];

  const seedMut = useMutation({
    mutationFn: () => ipc.onboarding.seedDemo(library.id),
    onSuccess: onDone,
  });

  const consentMut = useMutation({
    mutationFn: (granted: boolean) => ipc.telemetry.consent.set(granted),
  });

  const next = () => setStep((n) => Math.min(n + 1, STEPS.length - 1));

  /**
   * Answering moves on either way — including when the write fails.
   *
   * A first run that gets stuck on a privacy question because sqlite hiccuped
   * would be a terrible first impression, and the cost of moving on is that the
   * record stays absent: nothing is sent, and the corner card asks again later.
   * Failing towards "not asked" is the same direction every other consent path
   * fails in.
   */
  const answerConsent = (granted: boolean) => {
    consentMut.mutate(granted, { onSuccess: next, onError: next });
  };

  return (
    <div className="grid h-screen w-screen place-items-center bg-[var(--color-bg)] text-[var(--color-fg)]">
      <div className="w-full max-w-lg px-8 text-center">
        <div className="mx-auto mb-6 grid h-14 w-14 place-items-center rounded-2xl bg-[var(--color-brand)] text-2xl font-bold text-[var(--color-accent)]">
          S
        </div>

        {current === "language" && (
          <>
            <h1 className="text-[var(--text-ui-3xl)] font-bold">
              {t("welcomeTitle")}
            </h1>
            <p className="mx-auto mt-2 max-w-md text-sm text-[var(--color-fg-muted)]">
              {t("welcomeIntro")}
            </p>

            <div className="mt-8 text-left">
              <p className="mb-2 text-xs font-semibold tracking-widest text-[var(--color-fg-muted)] uppercase">
                {t("pickLanguage")}
              </p>
              <div className="flex flex-wrap gap-2">
                {LANGS.map((l) => (
                  <button
                    key={l}
                    type="button"
                    onClick={() => setLang(l)}
                    className={cn(
                      "rounded-full border px-3 py-1.5 text-sm transition-colors",
                      lang === l
                        ? "border-[var(--color-accent)] bg-[var(--color-accent)] text-[var(--color-sunday-blue-900)]"
                        : "border-[var(--color-border)] text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)]",
                    )}
                  >
                    {langLabel(l)}
                  </button>
                ))}
              </div>
            </div>

            <div className="mt-8 flex justify-center gap-3">
              <button
                type="button"
                onClick={onDone}
                className="rounded-md border border-[var(--color-border)] px-4 py-2 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
              >
                {t("skip")}
              </button>
              <button
                type="button"
                onClick={next}
                className="rounded-md bg-[var(--color-accent)] px-5 py-2 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110"
              >
                {t("actionNext")}
              </button>
            </div>
          </>
        )}

        {current === "consent" && (
          <TelemetryConsentQuestion
            pending={consentMut.isPending}
            onAnswer={answerConsent}
          />
        )}

        {current === "finish" && (
          <>
            <h1 className="text-[var(--text-ui-3xl)] font-bold">
              {t("welcomeTitle")}
            </h1>
            <p className="mx-auto mt-2 max-w-md text-sm text-[var(--color-fg-muted)]">
              {t("welcomeIntro")}
            </p>
            <div className="mt-8 flex justify-center gap-3">
              <button
                type="button"
                onClick={onDone}
                className="rounded-md border border-[var(--color-border)] px-4 py-2 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
              >
                {t("skip")}
              </button>
              <button
                type="button"
                onClick={() => seedMut.mutate()}
                disabled={seedMut.isPending}
                className="flex items-center gap-2 rounded-md bg-[var(--color-accent)] px-5 py-2 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-50"
              >
                {seedMut.isPending ? (
                  <Play size={15} />
                ) : (
                  <Sparkles size={15} />
                )}
                {seedMut.isPending ? t("seeding") : t("seedDemo")}
              </button>
            </div>
            {seedMut.isError && (
              <p className="mt-3 text-xs text-[var(--color-danger)]">
                {String(seedMut.error)}
              </p>
            )}
          </>
        )}

        {/* Step dots, as in the tutorial overlay: where you are, nothing more. */}
        <div className="mt-8 flex justify-center gap-1.5">
          {STEPS.map((s, i) => (
            <span
              key={s}
              className={
                i === step
                  ? "h-1.5 w-4 rounded-full bg-[var(--color-accent)]"
                  : "h-1.5 w-1.5 rounded-full bg-[var(--color-border)]"
              }
            />
          ))}
        </div>
      </div>
    </div>
  );
}
