/**
 * Phase 4.1 — one-time AI consent dialog.
 *
 * Shown the first time a feature would send content to Anthropic. Spells out
 * exactly what leaves the device so the choice is informed.
 */
import { Dialog } from "@/components/ui";
import { Button } from "@/components/ui";

interface Props {
  open: boolean;
  onAccept: () => void;
  onClose: () => void;
}

export function ConsentDialog({ open, onAccept, onClose }: Props) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Bruke AI-funksjoner?"
      description="AI-funksjoner sender innhold til Anthropic (Claude) for behandling."
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Avbryt
          </Button>
          <Button onClick={onAccept}>Godta og fortsett</Button>
        </>
      }
    >
      <div className="space-y-3 text-sm text-[var(--color-fg-muted)]">
        <p>Når du bruker en AI-funksjon, sendes følgende til Anthropic:</p>
        <ul className="list-disc space-y-1 pl-5">
          <li>
            teksten du ber om å få behandlet (f.eks. limt-inn lyrikk eller en
            planleggings-beskrivelse)
          </li>
          <li>
            ingen sanger, tjenester eller medier utover det den enkelte
            handlingen trenger
          </li>
        </ul>
        <p>
          API-nøkkelen din lagres i systemets nøkkelring, aldri i klartekst.
          Funksjoner med lokal fallback (som lyrikkformatering) virker uten AI.
          Du kan trekke samtykket tilbake i Innstillinger.
        </p>
      </div>
    </Dialog>
  );
}
