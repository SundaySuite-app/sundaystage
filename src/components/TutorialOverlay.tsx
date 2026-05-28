/**
 * Phase 13.1 — first-run interactive tutorial.
 *
 * A short 5-step tour (library → editor → AI → live → command palette) shown
 * once after onboarding. Deliberately not anchored to specific DOM nodes — a
 * calm corner card with Next/Skip that survives layout changes — so it never
 * points at the wrong thing.
 */
import { useState, type ReactNode } from "react";
import {
  Library,
  Pencil,
  Sparkles,
  Play,
  Command as CommandIcon,
  type LucideIcon,
} from "lucide-react";

import { Button } from "@/components/ui";

interface Step {
  icon: LucideIcon;
  title: string;
  body: ReactNode;
}

const STEPS: Step[] = [
  {
    icon: Library,
    title: "Sangbiblioteket",
    body: "Alle sangene dine bor her. Søk i tekstlinjer, filtrer på språk eller lisens, og se forhåndsvisning til høyre. Vi har lagt inn et lite startbibliotek du kan leke med.",
  },
  {
    icon: Pencil,
    title: "Rediger en sang",
    body: "Dobbeltklikk en sang for å åpne editoren — del opp i vers/refreng og bygg arrangementer. Lysbildene genereres automatisk fra seksjonene.",
  },
  {
    icon: Sparkles,
    title: "La AI gjøre det kjedelige",
    body: "Lim inn rå lyrikk og trykk «Formater» — AI strukturerer vers, refreng og arrangement. Uten API-nøkkel formateres det lokalt. Legg inn nøkkel under Innstillinger.",
  },
  {
    icon: Play,
    title: "Gå live",
    body: "Trykk «Gå live» nede til venstre. Cue-listen kjøres med piltastene; Esc = blackout, L = logo. Koble til en projektor under «Skjermer».",
  },
  {
    icon: CommandIcon,
    title: "Søk overalt med ⌘K",
    body: "Trykk ⌘K hvor som helst for å hoppe mellom sider eller søke på tvers av sanger, bibelvers og tjenester.",
  },
];

export function TutorialOverlay({ onDone }: { onDone: () => void }) {
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
        <h2 className="font-semibold">{s.title}</h2>
      </div>
      <p className="text-sm leading-relaxed text-[var(--color-fg-muted)]">
        {s.body}
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
            Hopp over
          </button>
          <Button
            size="sm"
            onClick={() => (last ? onDone() : setStep((n) => n + 1))}
          >
            {last ? "Ferdig" : "Neste"}
          </Button>
        </div>
      </div>
    </div>
  );
}
