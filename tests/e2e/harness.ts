/**
 * A fake Tauri host for the Playwright specs.
 *
 * The e2e suite runs the BUILT frontend in a plain browser, where
 * `@tauri-apps/api`'s `invoke` reaches for `window.__TAURI_INTERNALS__` and
 * finds nothing. Installing a stub there — before any bundle code runs — turns
 * the real UI into something a browser can drive, without a Tauri runtime and
 * without touching the components under test.
 *
 * Deliberately a small in-memory backend rather than a per-call mock: the E6
 * consent flows are stateful (answering changes what `consent_get` returns, and
 * the whole point of the card is that it disappears once answered), and a stub
 * that always replied the same thing could not tell a working round-trip from a
 * button that does nothing.
 *
 * Unknown commands resolve to `null` rather than rejecting: the workspace makes
 * a dozen calls this suite does not care about, and a rejection storm would
 * bury the assertion that matters.
 */
import type { Page } from "@playwright/test";

export interface HarnessOptions {
  /**
   * The consent record the app starts with. `never-asked` is a fresh install
   * (and every pre-E6 install); pass `granted`/`denied` to start answered.
   */
  consent?: "never-asked" | "granted" | "denied";
  /** Seed the log tail the report dialog previews. */
  logTail?: string;
  /** Whether onboarding has already been completed on this "machine". */
  onboarded?: boolean;
}

/**
 * Install the fake host. Must be called BEFORE `page.goto`.
 *
 * Everything the stub does lives in the page, so the assertions read the same
 * state the UI does — `readCalls(page)` afterwards is how a spec checks that a
 * click actually reached the backend.
 */
export async function installTauriHarness(
  page: Page,
  options: HarnessOptions = {},
): Promise<void> {
  const { consent = "never-asked", logTail = "", onboarded = false } = options;

  await page.addInitScript(
    ({
      consent,
      logTail,
      onboarded,
    }: Required<Pick<HarnessOptions, "consent" | "logTail" | "onboarded">>) => {
      // The app's own first-run gates live in localStorage; set them here so a
      // spec can start either at onboarding or at the workspace.
      try {
        if (onboarded) {
          localStorage.setItem("ss-onboarded", "1");
          localStorage.setItem("ss-tutorial-done", "1");
        } else {
          localStorage.removeItem("ss-onboarded");
          localStorage.removeItem("ss-tutorial-done");
        }
        localStorage.setItem("ss-locale", "no");
      } catch {
        /* a browser without storage is still worth booting */
      }

      const CONSENT_VERSION = 1;
      const state = {
        status: consent as "never-asked" | "granted" | "denied",
        installId:
          consent === "granted" ? "11111111-2222-3333-4444-555555555555" : null,
        pendingReports: 0,
        calls: [] as Array<{ cmd: string; args: unknown }>,
      };

      const consentView = () => ({
        status: state.status,
        version: state.status === "never-asked" ? 0 : CONSENT_VERSION,
        decidedAt: state.status === "never-asked" ? null : 1_800_000_000_000,
        currentVersion: CONSENT_VERSION,
        needsPrompt: state.status === "never-asked",
        active: state.status === "granted",
      });

      const library = {
        id: "lib-1",
        name: "Personal",
        default_locale: "no",
        default_theme_id: null,
        default_template_id: null,
        created_at: 0,
        updated_at: 0,
      };

      const handlers: Record<
        string,
        (args: Record<string, unknown>) => unknown
      > = {
        // ── App shell ──────────────────────────────────────────────────────
        library_list: () => [library],
        library_create: () => library,
        service_upcoming: () => [],
        live_stage_presets: () => [],
        live_state: () => null,
        sync_status: () => "local_only",
        output_appearance: () => ({
          text_scale: 1.0,
          text_color: "#ffffff",
          bg_color: "#0a1730",
          h_align: "center",
          show_section_label: true,
          uppercase: false,
          line_height: 1.1,
        }),
        onboarding_seed_demo: () => ({ services: 1, songs: 3 }),

        // ── Telemetry: the E6 surface ──────────────────────────────────────
        telemetry_consent_get: () => consentView(),
        telemetry_consent_set: (args) => {
          state.status = args.granted ? "granted" : "denied";
          if (args.granted && !state.installId) {
            state.installId = "11111111-2222-3333-4444-555555555555";
          }
          return consentView();
        },
        telemetry_install_id: () => state.installId,
        telemetry_queue_status: () => ({
          pending: 0,
          failed: 0,
          oldestAt: null,
          lastError: null,
          pendingReports: state.pendingReports,
        }),
        telemetry_preview_payload: () => ({
          json: JSON.stringify(
            { schema: 1, installId: state.installId, counters: [] },
            null,
            2,
          ),
          isNextPayload: state.status === "granted",
          isEmpty: false,
        }),
        telemetry_report_preview: (args) => ({
          at: 1_800_000_000_000,
          context: args.context,
          message: String(args.message ?? ""),
          logTail,
        }),
        telemetry_report_submit: () => {
          state.pendingReports += 1;
          return state.status === "granted" ? "queued" : "sent";
        },
        telemetry_set_language: () => null,
        telemetry_counters: () => [],
        telemetry_quality_recent: () => [],
        crash_reports_count: () => 0,
        crash_report_frontend: () => null,

        // ── Update ring (the Advanced tab renders it above the card) ───────
        update_channel_get: () => "stable",
        update_check: () => null,

        // ── AI settings tab ────────────────────────────────────────────────
        ai_key_status: () => ({ stored: false, env: false }),
        ai_models: () => [],
      };

      const w = window as unknown as {
        __TAURI_INTERNALS__: Record<string, unknown>;
        __harness: typeof state;
      };
      w.__harness = state;
      w.__TAURI_INTERNALS__ = {
        // The event plugin calls this when a listener is registered.
        transformCallback: (cb: unknown) => {
          const id = Math.floor(Math.random() * 1e9);
          (window as unknown as Record<string, unknown>)[`_${id}`] = cb;
          return id;
        },
        invoke: (cmd: string, args: Record<string, unknown> = {}) => {
          state.calls.push({ cmd, args });
          const handler = handlers[cmd];
          return Promise.resolve(handler ? handler(args ?? {}) : null);
        },
      };
    },
    { consent, logTail, onboarded },
  );
}

/** Every `invoke` the page has made, in order. */
export async function readCalls(
  page: Page,
): Promise<Array<{ cmd: string; args: Record<string, unknown> }>> {
  return page.evaluate(
    () =>
      (
        window as unknown as {
          __harness: {
            calls: Array<{ cmd: string; args: Record<string, unknown> }>;
          };
        }
      ).__harness.calls,
  );
}
