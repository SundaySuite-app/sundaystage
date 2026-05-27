import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { Sidebar, type Route } from "@/components/Sidebar";
import { CommandPalette } from "@/components/CommandPalette";
import { LibraryPage } from "@/features/library/LibraryPage";
import { ipc } from "@/lib/ipc";
import type { Library } from "@/lib/bindings";

function App() {
  const [route, setRoute] = useState<Route>("library");

  // Auto-create a "Personal" library on first run so the UI has something
  // to point at. Phase 13 replaces this with the proper onboarding wizard.
  const librariesQuery = useQuery({
    queryKey: ["libraries"],
    queryFn:  () => ipc.library.list(),
  });

  useEffect(() => {
    if (librariesQuery.data && librariesQuery.data.length === 0) {
      void ipc.library
        .create({ name: "Personal", default_locale: "no" })
        .then(() => librariesQuery.refetch());
    }
  }, [librariesQuery.data, librariesQuery]);

  const activeLibrary: Library | undefined = librariesQuery.data?.[0];

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-[var(--color-bg)] text-[var(--color-fg)]">
      <Sidebar
        current={route}
        onNavigate={setRoute}
        onGoLive={() => alert("Live mode not yet implemented — Phase 5")}
      />

      <main className="flex-1 overflow-hidden">
        {!activeLibrary ? (
          <div className="grid h-full place-items-center text-[var(--color-fg-muted)]">
            <p>Laster bibliotek…</p>
          </div>
        ) : route === "library" ? (
          <LibraryPage library={activeLibrary} />
        ) : (
          <Placeholder route={route} />
        )}
      </main>

      <CommandPalette onNavigate={setRoute} />
    </div>
  );
}

function Placeholder({ route }: { route: Route }) {
  const titles: Record<Route, { title: string; phase: string }> = {
    dashboard: { title: "Dashbord",       phase: "Phase 2.1" },
    library:   { title: "Sangbibliotek",  phase: "Phase 2.2" },
    services:  { title: "Tjenester",      phase: "Phase 3" },
    bible:     { title: "Bibel",          phase: "Phase 7.1" },
    media:     { title: "Media",          phase: "Phase 7.2" },
    settings:  { title: "Innstillinger",  phase: "Phase 13" },
  };
  const info = titles[route];

  return (
    <div className="grid h-full place-items-center">
      <div className="text-center max-w-sm">
        <div className="text-xs font-medium uppercase tracking-widest text-[var(--color-accent)] mb-2">
          {info.phase}
        </div>
        <h1 className="text-[var(--text-ui-2xl)] font-bold mb-2">{info.title}</h1>
        <p className="text-sm text-[var(--color-fg-muted)]">
          Denne siden er planlagt for {info.phase}. Vi har scaffolding klar —
          implementasjon kommer i senere fase.
        </p>
        <p className="mt-6 text-xs text-[var(--color-fg-muted)]">
          Trykk{" "}
          <kbd className="rounded border border-[var(--color-border)] bg-[var(--color-bg-surface)] px-1.5 py-0.5 font-mono">
            ⌘K
          </kbd>{" "}
          for kommandopaletten.
        </p>
      </div>
    </div>
  );
}

export default App;
