/**
 * Service-planning assistant — Phase 11.2.
 *
 * Describe a service in words; Claude proposes songs from *this* library plus
 * readings and transitions; the user reviews and creates a real service. The
 * AI can only pick songs the church owns (validated server-side), so an
 * accepted plan never references an invented song. Planning needs an API key —
 * there's no offline fallback.
 */

import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Music, Sparkles, X } from "lucide-react";

import type { Library, PlanItem, ServicePlan } from "@/lib/bindings";
import { ipc } from "@/lib/ipc";
import { hasAiConsent, grantAiConsent, preferredModel } from "@/lib/aiConsent";
import { ConsentDialog } from "@/components/ConsentDialog";

interface PlanModalProps {
  library: Library;
  onClose: () => void;
  onCreated: (serviceName: string) => void;
}

export function PlanModal({ library, onClose, onCreated }: PlanModalProps) {
  const [brief, setBrief] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState(preferredModel() ?? "claude-sonnet-4-6");
  const [plan, setPlan] = useState<ServicePlan | null>(null);
  const [consentOpen, setConsentOpen] = useState(false);

  const modelsQuery = useQuery({
    queryKey: ["aiModels"],
    queryFn: () => ipc.ai.models(),
  });

  const planMut = useMutation({
    mutationFn: () =>
      ipc.ai.planService(library.id, brief, apiKey.trim() || null, model),
    onSuccess: setPlan,
  });

  // Planning always hits the cloud, so always gate on consent.
  function attemptPlan() {
    if (!hasAiConsent()) setConsentOpen(true);
    else planMut.mutate();
  }
  const applyMut = useMutation({
    mutationFn: (p: ServicePlan) => ipc.ai.applyPlan(library.id, p),
    onSuccess: (svc) => {
      onCreated(svc.name);
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
      <div className="relative flex max-h-[85vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] shadow-[var(--shadow-elevated)]">
        <header className="flex items-center gap-2 border-b border-[var(--color-border)] px-4 py-3">
          <Sparkles size={16} className="text-[var(--color-accent)]" />
          <h2 className="font-semibold">Planlegg tjeneste med AI</h2>
          <div className="flex-1" />
          <button
            type="button"
            onClick={onClose}
            className="grid h-7 w-7 place-items-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            <X size={15} />
          </button>
        </header>

        <div className="space-y-3 border-b border-[var(--color-border)] p-4">
          <textarea
            value={brief}
            onChange={(e) => setBrief(e.target.value)}
            rows={3}
            placeholder="F.eks.: 25 min lovsang om tilgivelse for en ungdomsgudstjeneste, rolig avslutning."
            className="w-full resize-none rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] p-3 text-sm focus:border-[var(--color-accent)] focus:outline-none"
          />
          <div className="flex items-center gap-2">
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="Anthropic API-nøkkel (påkrevd)"
              className="flex-1 rounded-md border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-2 py-1.5 text-xs focus:border-[var(--color-accent)] focus:outline-none"
            />
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
              onClick={attemptPlan}
              disabled={brief.trim().length === 0 || planMut.isPending}
              className="flex items-center gap-1.5 rounded-md bg-[var(--color-brand)] px-3 py-1.5 text-sm font-medium text-white hover:brightness-110 disabled:opacity-50"
            >
              <Sparkles size={14} />
              {planMut.isPending ? "Tenker…" : "Foreslå plan"}
            </button>
          </div>
          {planMut.isError && (
            <p className="text-xs text-[var(--color-danger)]">
              {String(planMut.error)}
            </p>
          )}
        </div>

        <div className="flex-1 overflow-y-auto p-4">
          {!plan ? (
            <p className="text-sm text-[var(--color-fg-muted)]">
              Beskriv tjenesten over, så foreslår AI sanger fra biblioteket
              ditt, lesninger og overganger.
            </p>
          ) : (
            <div className="space-y-3">
              <div>
                <h3 className="font-semibold">{plan.title}</h3>
                {plan.theme && (
                  <p className="text-xs text-[var(--color-fg-muted)]">
                    Tema: {plan.theme}
                  </p>
                )}
              </div>
              <ol className="space-y-1.5">
                {plan.items.map((item, i) => (
                  <PlanRow key={i} item={item} n={i + 1} />
                ))}
              </ol>
              {plan.warnings.length > 0 && (
                <ul className="space-y-1 border-t border-[var(--color-border)] pt-2">
                  {plan.warnings.map((w, i) => (
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
          )}
        </div>

        <footer className="flex items-center justify-end gap-2 border-t border-[var(--color-border)] px-4 py-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            Avbryt
          </button>
          <button
            type="button"
            onClick={() => plan && applyMut.mutate(plan)}
            disabled={!plan || applyMut.isPending}
            className="rounded-md bg-[var(--color-accent)] px-4 py-1.5 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110 disabled:opacity-40"
          >
            {applyMut.isPending ? "Oppretter…" : "Opprett tjeneste"}
          </button>
        </footer>
      </div>

      <ConsentDialog
        open={consentOpen}
        onClose={() => setConsentOpen(false)}
        onAccept={() => {
          grantAiConsent();
          setConsentOpen(false);
          planMut.mutate();
        }}
      />
    </div>
  );
}

function PlanRow({ item, n }: { item: PlanItem; n: number }) {
  const badge =
    item.kind === "song"
      ? "Sang"
      : item.kind === "scripture"
        ? "Skrift"
        : "Notat";
  return (
    <li className="flex items-center gap-3 rounded-md border border-[var(--color-border)] px-3 py-2 text-sm">
      <span className="w-5 font-mono text-[10px] text-[var(--color-fg-muted)]">
        {n}
      </span>
      <span className="rounded-full bg-[var(--color-bg-surface)] px-2 py-0.5 text-[10px] text-[var(--color-fg-muted)]">
        {badge}
      </span>
      <span className="flex-1">{item.title}</span>
      {item.kind === "song" && item.song_id && (
        <Music size={13} className="text-[var(--color-accent)]" />
      )}
      {item.key && (
        <span className="rounded bg-[var(--color-bg-surface)] px-1.5 py-0.5 font-mono text-[10px]">
          {item.key}
        </span>
      )}
      {item.reference && (
        <span className="text-xs text-[var(--color-fg-muted)]">
          {item.reference}
        </span>
      )}
    </li>
  );
}
