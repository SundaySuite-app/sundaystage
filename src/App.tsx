import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { UpdateBanner } from "@/components/UpdateBanner";
import { TutorialOverlay } from "@/components/TutorialOverlay";
import { hasSeenTutorial, markTutorialSeen } from "@/lib/tutorial";
import { OperatorWorkspace } from "@/features/workspace/OperatorWorkspace";
import { WelcomeScreen } from "@/features/onboarding/WelcomeScreen";
import { ipc } from "@/lib/ipc";
import { useT } from "@/lib/i18n";
import type { Library } from "@/lib/bindings";

const ONBOARDED_KEY = "ss-onboarded";

function App() {
  const [tutorialDone, setTutorialDone] = useState(() => hasSeenTutorial());
  const [onboarded, setOnboarded] = useState(() => {
    try {
      return localStorage.getItem(ONBOARDED_KEY) === "1";
    } catch {
      return true;
    }
  });
  const t = useT();
  const qc = useQueryClient();

  const finishOnboarding = () => {
    try {
      localStorage.setItem(ONBOARDED_KEY, "1");
    } catch {
      /* ignore */
    }
    setOnboarded(true);
    void qc.invalidateQueries();
  };

  // Auto-create a "Personal" library on first run so the UI has something to
  // point at. Phase 13 replaces this with the proper onboarding wizard.
  const librariesQuery = useQuery({
    queryKey: ["libraries"],
    queryFn: () => ipc.library.list(),
  });

  useEffect(() => {
    if (librariesQuery.data && librariesQuery.data.length === 0) {
      void ipc.library
        .create({ name: "Personal", default_locale: "no" })
        .then(() => librariesQuery.refetch());
    }
  }, [librariesQuery.data, librariesQuery]);

  const activeLibrary: Library | undefined = librariesQuery.data?.[0];

  // First-run onboarding takes over until completed.
  if (!onboarded && activeLibrary) {
    return <WelcomeScreen library={activeLibrary} onDone={finishOnboarding} />;
  }

  if (!activeLibrary) {
    return (
      <div className="grid h-screen w-screen place-items-center bg-[var(--color-bg)] text-[var(--color-fg-muted)]">
        <p>{t("loadingLibrary")}</p>
      </div>
    );
  }

  return (
    <>
      <OperatorWorkspace library={activeLibrary} />
      <UpdateBanner />
      {!tutorialDone && (
        <TutorialOverlay
          onDone={() => {
            markTutorialSeen();
            setTutorialDone(true);
          }}
        />
      )}
    </>
  );
}

export default App;
