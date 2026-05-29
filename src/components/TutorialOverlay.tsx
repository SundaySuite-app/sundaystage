/**
 * Phase 13.1 — first-run interactive tutorial.
 *
 * A short 5-step tour (library → editor → AI → live → command palette) shown
 * once after onboarding. Deliberately not anchored to specific DOM nodes — a
 * calm corner card with Next/Skip that survives layout changes — so it never
 * points at the wrong thing.
 */
import { useState } from "react";
import {
  Library,
  Pencil,
  Sparkles,
  Play,
  Command as CommandIcon,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@/components/ui";
import { useT, type TKey } from "@/lib/i18n";

interface Step {
  icon: LucideIcon;
  titleKey: TKey;
  bodyKey: TKey;
}

const STEPS: Step[] = [
  { icon: Library, titleKey: "tutLibraryTitle", bodyKey: "tutLibraryBody" },
  { icon: Pencil, titleKey: "tutEditTitle", bodyKey: "tutEditBody" },
  { icon: Sparkles, titleKey: "tutAiTitle", bodyKey: "tutAiBody" },
  { icon: Play, titleKey: "tutLiveTitle", bodyKey: "tutLiveBody" },
  { icon: CommandIcon, titleKey: "tutSearchTitle", bodyKey: "tutSearchBody" },
];

export function TutorialOverlay({ onDone }: { onDone: () => void }) {
  const t = useT();
  const [step, setStep] = useState(0);
  const s = STEPS[step];
  const Icon = s.icon;
  const last = step === STEPS.length - 1;

  return (
    <div className="fixed right-6 bottom-6 z-50 w-[min(92vw,380px)] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-[var(--shadow-elevated)]">
      <div className="mb-3 flex items-center gap-2.5">
        <div className="grid h-8 w-8 place-items-center rounded-lg bg-[var(--color-accent)]/15 text-[var(--color-accent)]">
          <Icon size={16} />
        </div>
        <h2 className="font-semibold">{t(s.titleKey)}</h2>
      </div>
      <p className="text-sm leading-relaxed text-[var(--color-fg-muted)]">
        {t(s.bodyKey)}
      </p>

      <div className="mt-4 flex items-center justify-between">
        <div className="flex gap-1.5">
          {STEPS.map((_, i) => (
            <span
              key={i}
              className={
                i === step
                  ? "h-1.5 w-4 rounded-full bg-[var(--color-accent)]"
                  : "h-1.5 w-1.5 rounded-full bg-[var(--color-border)]"
              }
            />
          ))}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onDone}
            className="text-xs text-[var(--color-fg-muted)] hover:text-[var(--color-fg)]"
          >
            {t("actionSkip")}
          </button>
          <Button
            size="sm"
            onClick={() => (last ? onDone() : setStep((n) => n + 1))}
          >
            {last ? t("actionDone") : t("actionNext")}
          </Button>
        </div>
      </div>
    </div>
  );
}
