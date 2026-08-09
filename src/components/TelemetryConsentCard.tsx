/**
 * E6 — the consent question for installs that finished onboarding years ago.
 *
 * SundayStage has shipped since v0.1, so most machines will never see the
 * onboarding step again. Without this card the whole feature would only ever
 * reach fresh installs — which is exactly the failure the three-state consent
 * record was built to avoid (`NeverAsked` is not `Denied`).
 *
 * ## What it must never be
 *
 * **Never a modal.** A locked owner decision, and the right one: the operator
 * may be four minutes from a service with a projector to check. It is a corner
 * card, the workspace stays fully usable behind it, and every keystroke still
 * reaches the console.
 *
 * **Never during a service.** `isLive` comes from the workspace's own session
 * state — the same `session` that drives the LIVE badge — so the card cannot
 * appear mid-cue. This is the UI half of the promise the privacy text makes in
 * as many words ("reports are never sent while a service is running") and of
 * law 1 in the programme.
 *
 * ## Dismissing is not answering
 *
 * The × writes NOTHING. The record stays absent, the state stays `NeverAsked`,
 * and the card returns on the next launch rather than in the next minute — a
 * dismissal means "not now", and treating it as "no" would record an answer the
 * operator never gave. The in-session suppression is deliberately renderer-local
 * (component state, not storage) so it evaporates with the process.
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";

import { TelemetryConsentQuestion } from "@/components/TelemetryConsentQuestion";
import { ipc } from "@/lib/ipc";
import { useT } from "@/lib/i18n";
import { hasSeenTutorial } from "@/lib/tutorial";

export function TelemetryConsentCard({ isLive }: { isLive: boolean }) {
  const t = useT();
  const qc = useQueryClient();
  const [dismissed, setDismissed] = useState(false);
  // The tutorial overlay lives in the same corner. Read ONCE on mount rather
  // than subscribed: an operator who finishes the tour is not owed a consent
  // card sliding in underneath their hand — they get it on the next launch,
  // which is the same "asked again later, never nagged" rule the × follows.
  const [tutorialWasDone] = useState(() => hasSeenTutorial());

  const consent = useQuery({
    queryKey: ["telemetryConsent"],
    queryFn: () => ipc.telemetry.consent.get(),
    // A machine with no Tauri host (or a database that will not open) must not
    // retry this forever behind an operator's back.
    retry: false,
  });

  const answer = useMutation({
    mutationFn: (granted: boolean) => ipc.telemetry.consent.set(granted),
    onSuccess: (next) => {
      qc.setQueryData(["telemetryConsent"], next);
      void qc.invalidateQueries({ queryKey: ["telemetryQueue"] });
    },
  });

  // Every gate, in the order that costs least: dismissed, live, mid-tour,
  // already answered.
  if (dismissed || isLive || !tutorialWasDone) return null;
  if (!consent.data?.needsPrompt) return null;

  return (
    <div
      role="region"
      aria-label={t("telConsentTitle")}
      className="fixed right-6 bottom-6 z-40 w-[min(92vw,380px)] rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] p-5 shadow-[var(--shadow-elevated)]"
    >
      <button
        type="button"
        onClick={() => setDismissed(true)}
        title={t("telConsentDismissLabel")}
        aria-label={t("telConsentDismissLabel")}
        className="absolute top-3 right-3 rounded-md p-1 text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
      >
        <X size={14} />
      </button>
      <TelemetryConsentQuestion
        compact
        pending={answer.isPending}
        onAnswer={(granted) => {
          answer.mutate(granted, {
            // Closing on success rather than on click: if the write fails the
            // card stays, because a question that vanished without recording an
            // answer would be asked again next launch as though nothing had
            // happened.
            onSuccess: () => setDismissed(true),
          });
        }}
      />
    </div>
  );
}
