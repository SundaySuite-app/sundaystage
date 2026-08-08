import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { UpdateBanner } from "@/components/UpdateBanner";
import { TutorialOverlay } from "@/components/TutorialOverlay";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { installErrorReporting } from "@/lib/errorReporting";
import { hasSeenTutorial, markTutorialSeen } from "@/lib/tutorial";
import { OperatorWorkspace } from "@/features/workspace/OperatorWorkspace";
import { WelcomeScreen } from "@/features/onboarding/WelcomeScreen";
import { ipc } from "@/lib/ipc";
import { useT } from "@/lib/i18n";
import type { Library } from "@/lib/bindings";

const ONBOARDED_KEY = "ss-onboarded";

function App() {
  // E3 — window.onerror + unhandledrejection into the same bounded, scrubbed
  // crash ring the Rust panic hook writes to. Installed once, before anything
  // else can throw; the function is idempotent because StrictMode double-runs
  // effects in development.
  useEffect(() => {
    installErrorReporting();
  }, []);

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
      <div className="grid h-screen w-screen place-items-center bg-[var(--color-bg)] text-[var(--color-fg)]">
        <div className="flex flex-col items-center gap-4 text-center">
          <div className="grid h-14 w-14 place-items-center rounded-2xl bg-[var(--color-brand)] text-2xl font-bold text-[var(--color-accent)]">
            S
          </div>
          <span className="text-[var(--text-ui-xl)] font-bold">
            SundayStage
          </span>
          <p className="text-sm text-[var(--color-fg-muted)]">
            {t("loadingLibrary")}
          </p>
        </div>
      </div>
    );
  }

  return (
    <>
      {/* A render error in the workspace used to leave a blank white screen
          with nothing in any log. It now leaves a record in the crash ring and
          a recovery card — while the projector, in its own process, is
          untouched. */}
      <ErrorBoundary component="OperatorWorkspace">
        <OperatorWorkspace library={activeLibrary} />
      </ErrorBoundary>
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
