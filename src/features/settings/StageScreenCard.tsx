/**
 * A5 — the one decision a church has to make about its stage screen.
 *
 * When the operator blacks out the congregation screen, does the stage screen
 * go dark too? Default no: the band is mid-song and blackout is aimed at the
 * room, not at the musicians. Churches running one shared screen for both roles
 * want the opposite, so it is a switch rather than a rule.
 *
 * A per-device preference (localStorage), like the locale and the theme — the
 * screens differ per building, not per library.
 */
import { Monitor } from "lucide-react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui";
import { useT } from "@/lib/i18n";
import { useStageSettings } from "@/lib/stageSettings";
import { ToggleRow } from "./ToggleRow";

export function StageScreenCard() {
  const t = useT();
  const followsBlackout = useStageSettings((s) => s.followsBlackout);
  const setFollowsBlackout = useStageSettings((s) => s.setFollowsBlackout);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Monitor size={16} aria-hidden />
          {t("setStageScreenTitle")}
        </CardTitle>
        <CardDescription>{t("setStageScreenDesc")}</CardDescription>
      </CardHeader>
      <CardContent>
        <ToggleRow
          label={t("setStageFollowsBlackout")}
          description={t("setStageFollowsBlackoutDesc")}
          checked={followsBlackout}
          onChange={setFollowsBlackout}
        />
      </CardContent>
    </Card>
  );
}
