/**
 * The unified operator workspace — SundayStage's convergence on the worship
 * console layout (ProPresenter / EasyWorship / FreeShow), done our way.
 *
 * One screen, always present:
 *   ┌ TransportBar ───────────────────────────────────────────────┐
 *   │ ScheduleRail │        SlideGrid          │ PreviewLivePanel   │
 *   └ MediaDrawer ─────────────────────────────────────────────────┘
 * Library/Media are summoned in (progressive disclosure) so the resting state
 * stays three clean columns — a volunteer can run it after ten minutes.
 *
 * The Rust `LiveSession` stays the single source of truth for what's on air.
 * This component adds one frontend concept on top: `previewIndex` — the staged
 * slide. Clicking a slide stages it; "Go" promotes Preview → Live via the
 * existing `go_to` dispatch. Nothing reaches the projector without Go.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { ipc } from "@/lib/ipc";
import type {
  Cue,
  Library,
  LiveAction,
  LiveSessionView,
  OutputAppearance,
  Service,
  ServiceItem,
} from "@/lib/bindings";
import { useT } from "@/lib/i18n";
import { useErrorToast } from "@/lib/useErrorToast";
import { ErrorToast } from "@/components/ui";
import { DEFAULT_OUTPUT_APPEARANCE, useOutputBridge } from "@/lib/outputBridge";
import { useWebShare, type RemoteCommand } from "@/lib/webShare";
import { useLiveBridge, type LiveBridgeTransports } from "@/lib/useLiveBridge";
import {
  buildLiveBridgeContext,
  type BridgeCue,
  type LiveBridgeContext,
} from "@/lib/liveBridge";
import {
  CommandPalette,
  type PaletteAction,
  type Route,
} from "@/components/CommandPalette";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ServicesPage } from "@/features/services/ServicesPage";
import { StageDisplay } from "@/features/live/StageDisplay";
import { ExportModal } from "@/features/live/ExportModal";
import { TransportBar } from "./TransportBar";
import { ScheduleRail } from "./ScheduleRail";
import { SlideGrid } from "./SlideGrid";
import { PreviewLivePanel } from "./PreviewLivePanel";
import { LibraryBrowser, type BrowserTab } from "./LibraryBrowser";
import { MessagePanel } from "./MessagePanel";
import { MediaDrawer } from "./MediaDrawer";
import { JumpModal } from "./JumpModal";
import { ShortcutsModal } from "./ShortcutsModal";
import { cueServiceItemId, parseBibleRef } from "./cueUtils";
import { keyScope } from "./consoleKeys";
import { cn } from "@/lib/cn";
import { SingleFlight } from "./singleFlight";
import type { BibleDeepLink } from "@/features/bible/BiblePage";

export function OperatorWorkspace({ library }: { library: Library }) {
  const t = useT();
  const qc = useQueryClient();
  // Surfaces live-IPC failures that would otherwise be swallowed by silent
  // `.catch(() => {})` — a failed Go-Live or dispatch must reach the operator.
  const {
    message: ipcError,
    showError,
    dismiss: dismissError,
  } = useErrorToast();

  const [selectedServiceId, setSelectedServiceId] = useState<string | null>(
    null,
  );
  const [previewIndex, setPreviewIndex] = useState(0);
  const [session, setSession] = useState<LiveSessionView | null>(null);
  const [recoverable, setRecoverable] = useState<LiveSessionView | null>(null);
  // Per-session bridge context (Stage → Rec/Song), assembled at "Go Live".
  const [bridgeContext, setBridgeContext] = useState<LiveBridgeContext | null>(
    null,
  );

  // Overlays
  const [browser, setBrowser] = useState<{ tab: BrowserTab } | null>(null);
  const [browserSongId, setBrowserSongId] = useState<string | null>(null);
  const [bibleDeepLink, setBibleDeepLink] = useState<BibleDeepLink | null>(
    null,
  );
  const [mediaOpen, setMediaOpen] = useState(false);
  const [messageOpen, setMessageOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [scheduleEditorOpen, setScheduleEditorOpen] = useState(false);
  const [jumpOpen, setJumpOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [stageOpen, setStageOpen] = useState(false);
  const [stagePresetId, setStagePresetId] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

  const isLive = !!session;

  // Single-flight guard for go-live. `go()` decides whether to start a session
  // from the `session` React state, which only updates after `ipc.live.start()`
  // resolves — so two rapid Space/Enter/G presses (auto-repeat, double-tap)
  // would both see no session and both call `live_start`. The second call
  // truncates the first's crash-recovery WAL, jumps `started_at`, and re-zeroes
  // the companion seq. Routing every start through one in-flight promise makes
  // the round-trip idempotent under double-fire.
  const startFlight = useRef(new SingleFlight<LiveSessionView | null>());

  // ── Data ────────────────────────────────────────────────────────────────
  const servicesQuery = useQuery({
    queryKey: ["services", library.id],
    queryFn: () => ipc.service.upcoming(library.id, 0, 100),
  });
  const services = useMemo(
    () => servicesQuery.data ?? [],
    [servicesQuery.data],
  );
  const service: Service | null =
    services.find((s) => s.id === selectedServiceId) ?? services[0] ?? null;

  // Default-select the first service once the list loads.
  useEffect(() => {
    if (!selectedServiceId && services.length > 0)
      setSelectedServiceId(services[0].id);
  }, [services, selectedServiceId]);

  const cueListQuery = useQuery({
    queryKey: ["cueList", service?.id],
    queryFn: () => ipc.live.compileCueList(service!.id),
    enabled: !!service,
  });
  const cues: Cue[] = useMemo(
    () => cueListQuery.data?.cues ?? [],
    [cueListQuery.data],
  );

  // Live → SundaySong/Rec bridge (Phase 3 consumer). The driver is pure and the
  // transports default OFF, so it never touches the live output: it runs and is
  // fully tested, but until real transports are injected its events go nowhere.
  // `bridgeContext` is assembled at "Go Live" (it needs the per-item song map).
  const bridgeCues = useMemo<BridgeCue[]>(() => cues.map(toBridgeCue), [cues]);
  const bridgeTransports: LiveBridgeTransports = useMemo(() => ({}), []);
  const bridge = useLiveBridge(bridgeContext, bridgeCues, bridgeTransports);

  const summaryQuery = useQuery({
    queryKey: ["cueSummary", service?.id],
    queryFn: () => ipc.service.cueSummary(service!.id),
    enabled: !!service,
  });
  const itemTitles = useMemo(() => {
    const m = new Map<string, string>();
    for (const it of summaryQuery.data?.items ?? [])
      m.set(it.service_item_id, it.title);
    return m;
  }, [summaryQuery.data]);

  // service_item_id → first cue index, for "click an item → stage its start".
  const itemFirstIndex = useMemo(() => {
    const m = new Map<string, number>();
    cues.forEach((cue, i) => {
      const id = cueServiceItemId(cue);
      if (id && !m.has(id)) m.set(id, i);
    });
    return m;
  }, [cues]);

  const appearanceQuery = useQuery({
    queryKey: ["outputAppearance"],
    queryFn: () => ipc.output.appearance(),
  });
  const appearance: OutputAppearance =
    appearanceQuery.data ?? DEFAULT_OUTPUT_APPEARANCE;

  const stagePresetsQuery = useQuery({
    queryKey: ["stagePresets"],
    queryFn: () => ipc.live.stagePresets(),
  });
  const stagePresets = useMemo(
    () => stagePresetsQuery.data ?? [],
    [stagePresetsQuery.data],
  );
  const stagePreset =
    stagePresets.find((p) => p.id === stagePresetId) ?? stagePresets[0];

  // Reset preview when the service changes.
  useEffect(() => {
    setPreviewIndex(0);
  }, [service?.id]);

  // Keep preview in range as the cue list changes.
  const clampedPreview = Math.min(previewIndex, Math.max(0, cues.length - 1));
  useEffect(() => {
    if (clampedPreview !== previewIndex) setPreviewIndex(clampedPreview);
  }, [clampedPreview, previewIndex]);

  // Drive the projector windows from the live frame.
  useOutputBridge(session?.frame ?? null, isLive);

  // Crash recovery: detect a session that ended abnormally (Phase 6.1).
  useEffect(() => {
    ipc.live
      .recover()
      .then((v) => v && setRecoverable(v))
      .catch(() => {});
  }, []);

  const liveIndex = session?.index ?? null;
  const focusedItemId = cues[clampedPreview]
    ? cueServiceItemId(cues[clampedPreview])
    : null;

  // ── Live actions ──────────────────────────────────────────────────────────
  const dispatch = useCallback(
    (action: LiveAction) => {
      ipc.live
        .dispatch(action)
        .then((next) =>
          setSession((prev) => {
            // Diff against the *previous* index so the bridge sees exactly the
            // movement the operator made. Blackout/logo keep the index, so the
            // driver naturally emits nothing for them.
            if (prev) bridge.cueChange(prev.index, next.index);
            return next;
          }),
        )
        // A dropped dispatch means the projector didn't move — the operator must
        // know rather than press again into a void. Live output stays untouched.
        .catch(() => showError(t("dispatchError")));
    },
    [bridge, showError, t],
  );

  // ── Network share (stage.sundaysuite.app) ───────────────────────────────────
  // Forward each live frame to the web app so phones/extra screens follow, and
  // accept remote-control commands from a web operator → the same dispatcher.
  const onRemoteCommand = useCallback(
    (cmd: RemoteCommand) => {
      if (!isLive) return;
      const action: LiveAction =
        cmd === "next"
          ? { type: "next" }
          : cmd === "prev"
            ? { type: "previous" }
            : cmd === "black"
              ? { type: "blackout" }
              : cmd === "logo"
                ? { type: "show_logo" }
                : { type: "clear" };
      dispatch(action);
    },
    [isLive, dispatch],
  );
  // The upcoming cue's text, for the web scene/confidence monitor (/s).
  // Sensitive slides never leave the building — not even as a "next" preview.
  const webShareNext = useMemo(() => {
    if (session == null) return null;
    const nextCue = cues[session.index + 1];
    if (!nextCue || nextCue.kind !== "show_slide") return null;
    if (nextCue.slide_content.sensitive_slide) return null;
    return {
      lines: nextCue.slide_content.text_lines,
      label:
        nextCue.slide_content.section_label ?? nextCue.source.display_label,
    };
  }, [cues, session]);
  const webShare = useWebShare({
    frame: session?.frame ?? null,
    appearance,
    next: webShareNext,
    active: isLive,
    onCommand: onRemoteCommand,
  });
  // Stop sharing when the service ends.
  useEffect(() => {
    if (!isLive && webShare.status !== "off") void webShare.stop();
  }, [isLive, webShare]);

  const startSession = useCallback((): Promise<LiveSessionView | null> => {
    if (!service) return Promise.resolve(null);
    // Collapse concurrent go-live attempts into one live_start round-trip.
    return startFlight.current.run(
      async (): Promise<LiveSessionView | null> => {
        try {
          const v = await ipc.live.start(service.id);
          setSession(v);
          // Assemble the per-session bridge context: the planner already holds the
          // song behind each item — fetch the map and hand it to the driver so it
          // can report which song each cue showed. Best-effort: a failure here
          // leaves the bridge context null (driver no-ops) without blocking live.
          try {
            const songsByItem = await ipc.service.songsByItem(service.id);
            setBridgeContext(
              buildLiveBridgeContext(
                {
                  id: service.id,
                  library_id: service.library_id,
                  starts_at: Number(service.starts_at),
                },
                songsByItem,
              ),
            );
            bridge.goLive(v.index);
          } catch {
            /* usage map unavailable — bridge stays off, live proceeds */
          }
          // Open the projector window if a monitor is assigned (mirrors the old
          // console's auto-open). Best-effort; preview-only when no Tauri/displays.
          try {
            const cfg = await ipc.output.config();
            if (cfg.assignments.some((a) => a.role !== "off"))
              await ipc.output.open();
          } catch {
            /* not in Tauri / no external display */
          }
          void qc.invalidateQueries({ queryKey: ["services", library.id] });
          return v;
        } catch {
          // Go-Live failed: the projector never went live. Tell the operator
          // instead of silently leaving them staring at a dark screen.
          showError(t("lpStartError"));
          return null;
        }
      },
    );
  }, [service, qc, library.id, bridge, showError, t]);

  const stopSession = useCallback(() => {
    bridge.end();
    setBridgeContext(null);
    void ipc.live.end().finally(() => setSession(null));
  }, [bridge]);

  // Promote the staged slide to live, then stage the next one (worship flow).
  const go = useCallback(async () => {
    if (cues.length === 0) return;
    const target = clampedPreview;
    let s = session;
    if (!s) s = await startSession();
    if (!s) return;
    dispatch({ type: "go_to", index: target });
    setPreviewIndex(Math.min(target + 1, cues.length - 1));
  }, [cues.length, clampedPreview, session, startSession, dispatch]);

  const resumeRecovered = useCallback(async () => {
    if (!recoverable) return;
    setSelectedServiceId(recoverable.service_id);
    try {
      const v = await ipc.live.state();
      if (v) setSession(v);
    } finally {
      setRecoverable(null);
    }
  }, [recoverable]);

  // ── Hotkeys ─────────────────────────────────────────────────────────────
  // True modals own the keyboard entirely. The docked browser does NOT — the
  // whole point of docking it is that the operator can look up content while
  // the console stays armed (see consoleKeys.ts for the scoping rules).
  const modalOpen =
    settingsOpen ||
    scheduleEditorOpen ||
    jumpOpen ||
    shortcutsOpen ||
    stageOpen ||
    exportOpen;
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const scope = keyScope(e.target);
      // Never hijack typing in a form field.
      if (scope === "text") return;
      // A modal owns the keyboard — bail before every other case so console
      // shortcuts can't fire behind it. ⌘K is left untouched here so cmdk
      // still receives it.
      if (modalOpen) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        if (isLive) setJumpOpen((o) => !o);
        return;
      }
      // ⌘B toggles the resource browser (find a song → back, keyboard-only).
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setBrowser((b) => (b ? null : { tab: "songs" }));
        return;
      }
      if (e.metaKey || e.ctrlKey) return; // leave ⌘K etc. to their handlers
      // Esc closes the docked browser first; blackout keeps B (and Esc when
      // the browser is closed).
      if (e.key === "Escape" && browser) {
        e.preventDefault();
        setBrowser(null);
        return;
      }
      if (e.key === "?") {
        e.preventDefault();
        setShortcutsOpen((o) => !o);
        return;
      }
      // Focus inside the docked browser: only the panic keys reach the
      // console — Space/Enter/arrows keep doing browser navigation.
      if (scope === "dock") {
        switch (e.key) {
          case "b":
          case "B":
            if (isLive) {
              e.preventDefault();
              dispatch({ type: "blackout" });
            }
            break;
          case "l":
          case "L":
            if (isLive) dispatch({ type: "show_logo" });
            break;
        }
        return;
      }
      switch (e.key) {
        case "ArrowRight":
        case "ArrowDown":
        case "PageDown":
          e.preventDefault();
          setPreviewIndex((i) => Math.min(i + 1, cues.length - 1));
          break;
        case "ArrowLeft":
        case "ArrowUp":
        case "PageUp":
          e.preventDefault();
          setPreviewIndex((i) => Math.max(i - 1, 0));
          break;
        case " ":
        case "Enter":
        case "g":
        case "G":
          e.preventDefault();
          void go();
          break;
        case "Escape":
        case "b":
        case "B":
          if (isLive) {
            e.preventDefault();
            dispatch({ type: "blackout" });
          }
          break;
        case "l":
        case "L":
          if (isLive) dispatch({ type: "show_logo" });
          break;
        case "Home":
          setPreviewIndex(0);
          break;
        case "End":
          setPreviewIndex(Math.max(0, cues.length - 1));
          break;
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [modalOpen, browser, cues.length, go, dispatch, isLive]);

  // ── Command palette routing ───────────────────────────────────────────────
  const onNavigate = useCallback((route: Route) => {
    switch (route) {
      case "library":
        setBrowser({ tab: "songs" });
        break;
      case "bible":
        setBrowser({ tab: "scripture" });
        break;
      case "decks":
        setBrowser({ tab: "decks" });
        break;
      case "design":
        setBrowser({ tab: "themes" });
        break;
      case "media":
        setMediaOpen(true);
        break;
      case "services":
        setScheduleEditorOpen(true);
        break;
      case "settings":
        setSettingsOpen(true);
        break;
      default:
        break;
    }
  }, []);

  /** Palette quick actions — every advertised action does something real. */
  const onPaletteAction = useCallback(
    (action: PaletteAction) => {
      switch (action) {
        case "new-song":
          setBrowser({ tab: "songs" });
          break;
        case "new-service":
          setScheduleEditorOpen(true);
          break;
        case "go-live":
          if (!isLive && cues.length > 0) void startSession();
          break;
      }
    },
    [isLive, cues.length, startSession],
  );

  const onOpenResult = useCallback((route: Route, id: string) => {
    if (route === "services") {
      setSelectedServiceId(id);
    } else if (route === "library") {
      setBrowserSongId(id);
      setBrowser({ tab: "songs" });
    } else if (route === "bible") {
      // For bible hits `id` is the reference string ("John 3:16"); parse it so
      // the scripture browser opens that exact passage (matching openBibleCue).
      const ref = parseBibleRef(id);
      if (ref) {
        setBibleDeepLink({
          book: ref.book,
          chapter: ref.chapter,
          verseStart: ref.verseStart,
          verseEnd: ref.verseEnd,
        });
      }
      setBrowser({ tab: "scripture" });
    }
  }, []);

  /**
   * A bible passage was appended to the selected service from the browser.
   * Refresh the plan + grid, and when live hot-swap the running session's cue
   * list so the new cue is actually reachable — then either put it on air
   * ("Vis nå") or stage it in Preview.
   */
  const onBibleAdded = useCallback(
    async (item: ServiceItem, opts: { showNow: boolean }) => {
      if (!service) return;
      void qc.invalidateQueries({ queryKey: ["cueSummary", service.id] });
      // Refetch through the query cache so the grid and the index we compute
      // here see the exact same list.
      const list = await qc.fetchQuery({
        queryKey: ["cueList", service.id],
        queryFn: () => ipc.live.compileCueList(service.id),
      });
      const cueIndex = list.cues.findIndex(
        (c) => cueServiceItemId(c) === item.id,
      );
      // Hot-swap the live session's snapshot (it never recompiles by itself).
      // Only when the live session runs the service we just added to.
      if (session && session.service_id === service.id) {
        try {
          const view = await ipc.live.reload();
          setSession(view);
          if (opts.showNow && cueIndex >= 0) {
            dispatch({ type: "go_to", index: cueIndex });
            setPreviewIndex(Math.min(cueIndex + 1, list.cues.length - 1));
            return;
          }
        } catch {
          showError(t("dispatchError"));
          return;
        }
      }
      if (cueIndex >= 0) setPreviewIndex(cueIndex);
    },
    [service, session, qc, dispatch, showError, t],
  );

  /** Open the bible browser pre-navigated to the current preview cue's passage. */
  const openBibleCue = useCallback(() => {
    const cue = cues[clampedPreview];
    if (!cue || cue.kind !== "show_slide") return;
    const ref = parseBibleRef(cue.source.display_label);
    if (!ref) return;
    setBibleDeepLink({
      book: ref.book,
      chapter: ref.chapter,
      verseStart: ref.verseStart,
      verseEnd: ref.verseEnd,
    });
    setBrowser({ tab: "scripture" });
  }, [cues, clampedPreview]);

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-[var(--color-bg)] text-[var(--color-fg)]">
      <TransportBar
        services={services}
        service={service}
        onSelectService={setSelectedServiceId}
        onNewService={() => setScheduleEditorOpen(true)}
        onOpenBrowser={() => setBrowser({ tab: "songs" })}
        isLive={isLive}
        canGoLive={cues.length > 0}
        outputState={session?.output ?? null}
        onGoLive={() => void startSession()}
        onStop={stopSession}
        onBlackout={() => dispatch({ type: "blackout" })}
        onLogo={() => dispatch({ type: "show_logo" })}
        onMessage={() => isLive && setMessageOpen((o) => !o)}
        onClear={() => dispatch({ type: "clear" })}
        onJump={() => isLive && setJumpOpen(true)}
        onStage={() => isLive && setStageOpen(true)}
        onExport={() => isLive && setExportOpen(true)}
        onSettings={() => setSettingsOpen(true)}
        onShortcuts={() => setShortcutsOpen(true)}
      />

      {/* Operator message popover — a light popover, never a modal: the
          console keeps its keyboard while it is open. */}
      <MessagePanel
        open={messageOpen && isLive}
        active={session?.output === "message"}
        onShow={(text) => dispatch({ type: "show_message", text })}
        onClear={() => dispatch({ type: "clear" })}
        onClose={() => setMessageOpen(false)}
      />

      {isLive ? (
        <WebShareControl
          status={webShare.status}
          code={webShare.session?.code ?? null}
          onStart={() => void webShare.start()}
          onStop={() => void webShare.stop()}
        />
      ) : null}

      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* Docked resource browser: replaces the schedule column while open so
            the grid and Preview/Program never move — content lookup and live
            transport are no longer mutually exclusive. */}
        <LibraryBrowser
          library={library}
          open={!!browser}
          initialTab={browser?.tab ?? "songs"}
          openSongId={browserSongId}
          onDeepLinkDone={() => setBrowserSongId(null)}
          bibleDeepLink={bibleDeepLink}
          onBibleDeepLinkDone={() => setBibleDeepLink(null)}
          activeService={
            service ? { id: service.id, name: service.name } : null
          }
          isLive={isLive}
          onBibleAdded={onBibleAdded}
          onClose={() => setBrowser(null)}
        />
        {service ? (
          <div
            className={cn(
              "grid min-h-0 flex-1",
              browser ? "grid-cols-[1fr_340px]" : "grid-cols-[280px_1fr_340px]",
            )}
          >
            {!browser && (
              <ScheduleRail
                service={service}
                focusedItemId={focusedItemId}
                onFocusItem={(itemId) => {
                  const idx = itemFirstIndex.get(itemId);
                  if (idx != null) setPreviewIndex(idx);
                }}
                onEditSchedule={() => setScheduleEditorOpen(true)}
              />
            )}
            <main className="min-h-0 overflow-hidden bg-[var(--color-bg)]">
              <SlideGrid
                cues={cues}
                appearance={appearance}
                previewIndex={clampedPreview}
                liveIndex={liveIndex}
                itemTitles={itemTitles}
                onPreview={setPreviewIndex}
              />
            </main>
            <PreviewLivePanel
              cues={cues}
              appearance={appearance}
              previewIndex={clampedPreview}
              liveFrame={session?.frame ?? null}
              liveIndex={liveIndex}
              isLive={isLive}
              notes={service.notes}
              onGo={() => void go()}
              onOpenBibleCue={openBibleCue}
            />
          </div>
        ) : (
          <div className="grid flex-1 place-items-center text-center">
            <div className="max-w-sm">
              <h1 className="text-[var(--text-ui-2xl)] font-bold">
                {t("wsNoServiceTitle")}
              </h1>
              <p className="mt-2 text-sm text-[var(--color-fg-muted)]">
                {t("wsNoServiceBody")}
              </p>
              <button
                type="button"
                onClick={() => setScheduleEditorOpen(true)}
                className="mt-5 rounded-lg bg-[var(--color-accent)] px-4 py-2 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110"
              >
                {t("svcNewService")}
              </button>
            </div>
          </div>
        )}
      </div>

      <MediaDrawer
        library={library}
        open={mediaOpen}
        onToggle={() => setMediaOpen((o) => !o)}
      />

      {/* Overlays */}
      {scheduleEditorOpen && (
        <ModalShell
          onClose={() => setScheduleEditorOpen(false)}
          wide
          label={t("navServices")}
        >
          <ServicesPage
            library={library}
            openServiceId={service?.id ?? null}
            onGoLive={(svc) => {
              setSelectedServiceId(svc.id);
              setScheduleEditorOpen(false);
              void startSession();
            }}
          />
        </ModalShell>
      )}

      {settingsOpen && (
        <ModalShell
          onClose={() => setSettingsOpen(false)}
          wide
          label={t("navSettings")}
        >
          <SettingsPage />
        </ModalShell>
      )}

      {jumpOpen && (
        <JumpModal
          cues={cues}
          onPick={(i) => {
            setPreviewIndex(i);
            dispatch({ type: "go_to", index: i });
            setJumpOpen(false);
          }}
          onClose={() => setJumpOpen(false)}
        />
      )}

      {stageOpen && session && stagePreset && (
        <StageDisplay
          session={session}
          cues={cues}
          serviceName={service?.name ?? ""}
          notes={service?.notes ?? null}
          preset={stagePreset}
          presets={stagePresets}
          onPreset={setStagePresetId}
          onClose={() => setStageOpen(false)}
        />
      )}

      {exportOpen && <ExportModal onClose={() => setExportOpen(false)} />}

      {shortcutsOpen && (
        <ShortcutsModal onClose={() => setShortcutsOpen(false)} />
      )}

      {recoverable && (
        <RecoveryBanner
          session={recoverable}
          onResume={() => void resumeRecovered()}
          onDiscard={() => {
            void ipc.live.end();
            setRecoverable(null);
          }}
        />
      )}

      <CommandPalette
        onNavigate={onNavigate}
        onOpenResult={onOpenResult}
        onAction={onPaletteAction}
        libraryId={library.id}
      />

      <ErrorToast message={ipcError} onDismiss={dismissError} />
    </div>
  );
}

/** Project a compiled `Cue` onto the minimal shape the live bridge needs. */
function toBridgeCue(cue: Cue): BridgeCue {
  if (cue.kind === "show_slide") {
    return {
      serviceItemId: cue.source.service_item_id,
      displayLabel: cue.source.display_label,
      sectionLabel: cue.slide_content.section_label,
    };
  }
  // black_out / show_logo / pause carry no service item — non-song cues.
  return { serviceItemId: "", displayLabel: "", sectionLabel: null };
}

/** A centered modal shell that hosts a full-page feature reused as a dialog. */
function ModalShell({
  children,
  onClose,
  wide,
  label,
}: {
  children: React.ReactNode;
  onClose: () => void;
  wide?: boolean;
  label?: string;
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 grid place-items-center p-4 sm:p-8">
      <div
        className="absolute inset-0 bg-black/50"
        onClick={onClose}
        aria-hidden
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={label}
        className={
          "relative flex h-full max-h-[92vh] w-full flex-col overflow-hidden rounded-xl border border-[var(--color-border)] bg-[var(--color-bg)] shadow-[var(--shadow-elevated)] " +
          (wide ? "max-w-[1100px]" : "max-w-2xl")
        }
      >
        {children}
      </div>
    </div>
  );
}

function RecoveryBanner({
  session,
  onResume,
  onDiscard,
}: {
  session: LiveSessionView;
  onResume: () => void;
  onDiscard: () => void;
}) {
  const t = useT();
  return (
    <div className="fixed bottom-4 left-1/2 z-50 w-[min(90vw,560px)] -translate-x-1/2 rounded-xl border border-[var(--color-accent)]/40 bg-[var(--color-bg-elevated)] p-4 shadow-[var(--shadow-elevated)]">
      <p className="text-sm font-semibold">{t("recoveryTitle")}</p>
      <p className="mt-1 text-xs text-[var(--color-fg-muted)]">
        {t("recoveryBody", {
          index: session.index + 1,
          total: session.total,
        })}
      </p>
      <div className="mt-3 flex justify-end gap-2">
        <button
          type="button"
          onClick={onDiscard}
          className="rounded-md px-3 py-1.5 text-sm text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
        >
          {t("recoveryDiscard")}
        </button>
        <button
          type="button"
          onClick={onResume}
          className="rounded-md bg-[var(--color-accent)] px-4 py-1.5 text-sm font-bold text-[var(--color-sunday-blue-900)] hover:brightness-110"
        >
          {t("recoveryResume")}
        </button>
      </div>
    </div>
  );
}

/**
 * "Share over network" — a compact floating control shown while live. Starts a
 * SundayStage Web session and shows the 6-digit code the operator reads aloud
 * (or puts on the info screen) so phones and extra screens can follow at
 * stage.sundaysuite.app. Fully localized via the i18n catalog.
 */
function WebShareControl({
  status,
  code,
  onStart,
  onStop,
}: {
  status: "off" | "starting" | "sharing" | "error";
  code: string | null;
  onStart: () => void;
  onStop: () => void;
}) {
  const t = useT();
  const sharing = status === "sharing" || status === "starting";
  const statusLabel =
    status === "sharing"
      ? t("webShareStatusSharing")
      : status === "starting"
        ? t("webShareStatusStarting")
        : status === "error"
          ? t("webShareStatusError")
          : t("webShareStatusOff");
  return (
    <div className="pointer-events-auto fixed bottom-4 left-4 z-40 flex items-center gap-3 rounded-xl border border-[var(--color-border)] bg-[var(--color-bg-elevated)] px-3 py-2 shadow-[var(--shadow-elevated)]">
      {/* Screen-reader status: announce share state changes without cluttering
          the compact visual control. */}
      <span aria-live="polite" className="sr-only">
        {statusLabel}
      </span>
      {sharing && code ? (
        <>
          <span className="flex items-center gap-1.5 text-xs text-[var(--color-fg-muted)]">
            <span className="inline-block h-2 w-2 rounded-full bg-[var(--color-success,#5dbb78)]" />
            stage.sundaysuite.app
          </span>
          <span className="font-mono text-lg font-bold tracking-[0.2em] text-[var(--color-accent)]">
            {code}
          </span>
          <button
            onClick={onStop}
            className="rounded-md px-2 py-1 text-xs text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-surface)] hover:text-[var(--color-fg)]"
          >
            {t("webShareStop")}
          </button>
        </>
      ) : (
        <button
          onClick={onStart}
          disabled={status === "starting"}
          className="rounded-md px-2.5 py-1 text-xs font-semibold text-[var(--color-fg)] hover:bg-[var(--color-bg-surface)] disabled:opacity-50"
        >
          {status === "error" ? t("webShareRetry") : t("webShareStart")}
        </button>
      )}
    </div>
  );
}
