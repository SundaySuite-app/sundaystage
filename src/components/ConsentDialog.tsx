/**
 * Phase 4.1 — one-time AI consent dialog.
 *
 * Shown the first time a feature would send content to Anthropic. Spells out
 * exactly what leaves the device so the choice is informed.
 */
import { Dialog } from "@/components/ui";
import { Button } from "@/components/ui";
import { useT } from "@/lib/i18n";

interface Props {
  open: boolean;
  onAccept: () => void;
  onClose: () => void;
}

export function ConsentDialog({ open, onAccept, onClose }: Props) {
  const t = useT();
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("consentTitle")}
      description={t("consentDescription")}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("actionCancel")}
          </Button>
          <Button onClick={onAccept}>{t("consentAccept")}</Button>
        </>
      }
    >
      <div className="space-y-3 text-sm text-[var(--color-fg-muted)]">
        <p>{t("consentIntro")}</p>
        <ul className="list-disc space-y-1 pl-5">
          <li>{t("consentBullet1")}</li>
          <li>{t("consentBullet2")}</li>
        </ul>
        <p>{t("consentNote")}</p>
      </div>
    </Dialog>
  );
}
