/**
 * First-run welcome — Phase 13.1.
 *
 * Pick a language, then either seed a demo "Velkomstgudstjeneste" or start
 * empty. Shown once (gated by localStorage in App) when a fresh library has no
 * content yet.
 */

import { useMutation } from "@tanstack/react-query";
import { Play, Sparkles } from "lucide-react";

import type { Library } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { LANGS, langLabel, useLocale, useT } from "@/lib/i18n";
import { cn } from "@/lib/cn";

interface WelcomeScreenProps {
  library: Library;
  onDone: () => void;
}

export function WelcomeScreen({ library, onDone }: WelcomeScreenProps) {
  const t = useT();
  const lang = useLocale((s) => s.lang);
  const setLang = useLocale((s) => s.setLang);

  const seedMut = useMutation({
    mutationFn: () => ipc.onboarding.seedDemo(library.id),
    onSuccess: onDone,
  });

  return (
    <div className="grid h-screen w-screen place-items-center bg-[var(--color-bg)] text-[var(--color-fg)]">
      <div className="w-full max-w-lg px-8 text-center">
        <div className="mx-auto mb-6 grid h-14 w-14 place-items-center rounded-2xl bg-[var(--color-brand)] text-2xl font-bold text-[var(--color-accent)]">
          S
        </div>
        <h1 className="text-[var(--text-ui-3xl)] font-bold">
          {t("welcomeTitle")}
        </h1>
        <p className="mx-auto mt-2 max-w-md text-sm text-[var(--color-fg-muted)]">
          {t("welcomeIntro")}
        </p>

        <div className="mt-8 text-left">
          <p className="mb-2 text-xs font-semibold uppercase tracking-widest text-[var(--color-fg-muted)]">
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
            onClick={() => seedMut.mutate()}
            disabled={seedMut.isPending}
            className="flex items-center gap-2 rounded-md bg-[var(--color-accent)] px-5 py-2 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-50"
          >
            {seedMut.isPending ? <Play size={15} /> : <Sparkles size={15} />}
            {seedMut.isPending ? t("seeding") : t("seedDemo")}
          </button>
        </div>
        {seedMut.isError && (
          <p className="mt-3 text-xs text-[var(--color-danger)]">
            {String(seedMut.error)}
          </p>
        )}
      </div>
    </div>
  );
}
