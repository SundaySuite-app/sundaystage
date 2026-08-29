/**
 * E6 — the consent UX, driven through the real UI.
 *
 * Three things a unit test cannot prove, because they are about what an operator
 * can reach with a mouse:
 *
 *   1. both answers on the onboarding step lead ONWARD (a privacy question that
 *      traps a first run on "no" is worse than no question at all);
 *   2. the settings switch is a real round-trip — click, backend, badge;
 *   3. the report dialog opens from the command palette and its counter tracks
 *      what was typed.
 *
 * The Tauri host is stubbed in `harness.ts`: stateful, so answering actually
 * changes what the app is told next.
 */
import { test, expect } from "@playwright/test";

import { installTauriHarness, readCalls } from "./harness";

/** The Norwegian masters, as the catalog carries them. */
const CONSENT_TITLE = "Hjelp oss å gjøre SundayStage bedre?";
const YES = "Ja, del anonymt";
const NO = "Nei takk";
const REPORT_TITLE = "Rapporter et problem";

test.describe("onboarding consent step", () => {
  for (const [answer, label, granted] of [
    ["yes", YES, true],
    ["no", NO, false],
  ] as const) {
    test(`the ${answer} path records the answer and reaches the workspace`, async ({
      page,
    }) => {
      await installTauriHarness(page, { consent: "never-asked" });
      await page.goto("/");

      // Step 1: language.
      await expect(page.getByText("Norsk")).toBeVisible();
      await page.getByRole("button", { name: "Neste" }).click();

      // Step 2: the consent question, with both answers equally available.
      await expect(page.getByText(CONSENT_TITLE)).toBeVisible();
      await expect(page.getByRole("button", { name: YES })).toBeVisible();
      await expect(page.getByRole("button", { name: NO })).toBeVisible();

      // The scope is readable BEFORE answering.
      await page.getByRole("button", { name: "Hva sendes?" }).click();
      await expect(
        page.getByText("Krasj og feil", { exact: false }),
      ).toBeVisible();

      await page.getByRole("button", { name: label }).click();

      // Step 3: the demo-content step — and from there into the workspace.
      await expect(
        page.getByRole("button", { name: "Legg til demo-innhold" }),
      ).toBeVisible();
      await page.getByRole("button", { name: "Start tomt" }).click();
      await expect(page.getByText(CONSENT_TITLE)).toHaveCount(0);
      await expect(
        page.getByRole("button", { name: "Innstillinger" }),
      ).toBeVisible();

      const set = (await readCalls(page)).filter(
        (c) => c.cmd === "telemetry_consent_set",
      );
      expect(set).toHaveLength(1);
      expect(set[0].args.granted).toBe(granted);
    });
  }

  test("skipping onboarding records no answer at all", async ({ page }) => {
    // NeverAsked is not Denied: an operator who never reached the question must
    // be asked later by the corner card, not silently counted as "no".
    await installTauriHarness(page, { consent: "never-asked" });
    await page.goto("/");
    await page.getByRole("button", { name: "Start tomt" }).click();

    await expect(
      page.getByRole("button", { name: "Innstillinger" }),
    ).toBeVisible();
    expect(
      (await readCalls(page)).filter((c) => c.cmd === "telemetry_consent_set"),
    ).toHaveLength(0);
  });
});

test.describe("the corner card for existing installs", () => {
  test("asks an already-onboarded install, and answering dismisses it", async ({
    page,
  }) => {
    await installTauriHarness(page, {
      consent: "never-asked",
      onboarded: true,
    });
    await page.goto("/");

    const card = page.getByRole("region", { name: CONSENT_TITLE });
    await expect(card).toBeVisible();
    await card.getByRole("button", { name: NO }).click();
    await expect(card).toHaveCount(0);

    const set = (await readCalls(page)).filter(
      (c) => c.cmd === "telemetry_consent_set",
    );
    expect(set).toHaveLength(1);
    expect(set[0].args.granted).toBe(false);
  });

  test("stays away from an install that has already answered", async ({
    page,
  }) => {
    await installTauriHarness(page, { consent: "denied", onboarded: true });
    await page.goto("/");
    await expect(
      page.getByRole("button", { name: "Innstillinger" }),
    ).toBeVisible();
    await expect(page.getByRole("region", { name: CONSENT_TITLE })).toHaveCount(
      0,
    );
  });
});

test.describe("the privacy card", () => {
  /** Open Settings → Avansert, where the card lives. */
  async function openPrivacyCard(page: import("@playwright/test").Page) {
    await page.getByRole("button", { name: "Innstillinger" }).first().click();
    await page.getByRole("button", { name: "Avansert" }).click();
    await expect(page.getByText("Personvern", { exact: true })).toBeVisible();
  }

  test("the switch is a real round-trip", async ({ page }) => {
    await installTauriHarness(page, { consent: "denied", onboarded: true });
    await page.goto("/");
    await openPrivacyCard(page);

    const toggle = page.getByRole("switch", { name: "Del anonyme rapporter" });
    await expect(toggle).toHaveAttribute("aria-checked", "false");

    // On — with no confirmation step in the way.
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    // …and off again, with no dark pattern either.
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");

    const set = (await readCalls(page)).filter(
      (c) => c.cmd === "telemetry_consent_set",
    );
    expect(set.map((c) => c.args.granted)).toEqual([true, false]);
  });

  test("hard-crash capture is its own switch, and does not follow consent", async ({
    page,
  }) => {
    // A6. Two switches on one card is a real risk of confusion, so the thing
    // worth pinning is that they are genuinely independent: capture is local
    // (a diagnostic on this machine), consent is transmission. An operator who
    // says no to sharing must still be capturing crashes for their own support
    // conversation, and turning capture off must not read as consent to
    // anything.
    await installTauriHarness(page, { consent: "denied", onboarded: true });
    await page.goto("/");
    await openPrivacyCard(page);

    const sharing = page.getByRole("switch", { name: "Del anonyme rapporter" });
    const capture = page.getByRole("switch", { name: "Fang harde krasj" });

    // Sharing is off; capture is on anyway.
    await expect(sharing).toHaveAttribute("aria-checked", "false");
    await expect(capture).toHaveAttribute("aria-checked", "true");

    // Turning capture off leaves consent exactly where it was…
    await capture.click();
    await expect(capture).toHaveAttribute("aria-checked", "false");
    await expect(sharing).toHaveAttribute("aria-checked", "false");

    // …and turning sharing ON does not switch capture back on behind the
    // operator's back.
    await sharing.click();
    await expect(sharing).toHaveAttribute("aria-checked", "true");
    await expect(capture).toHaveAttribute("aria-checked", "false");

    const calls = await readCalls(page);
    expect(
      calls.filter((c) => c.cmd === "native_crash_set").map((c) => c.args),
    ).toEqual([{ enabled: false }]);
    expect(
      calls.filter((c) => c.cmd === "telemetry_consent_set").map((c) => c.args),
    ).toEqual([{ granted: true }]);
  });

  test("shows the real builder's bytes on request", async ({ page }) => {
    await installTauriHarness(page, { consent: "granted", onboarded: true });
    await page.goto("/");
    await openPrivacyCard(page);

    await expect(page.getByTestId("telemetry-payload-preview")).toHaveCount(0);
    await page.getByRole("button", { name: "Vis hva som sendes" }).click();
    await expect(page.getByTestId("telemetry-payload-preview")).toContainText(
      '"schema": 1',
    );
  });
});

test.describe("the problem-report dialog", () => {
  test("opens from the command palette and counts what was typed", async ({
    page,
  }) => {
    await installTauriHarness(page, {
      consent: "denied",
      onboarded: true,
      logTail: "12:00:01 INFO output child started",
    });
    await page.goto("/");
    // Wait for the workspace before pressing ⌘K: the palette's key listener is
    // installed on mount, and a keystroke sent before hydration goes nowhere.
    await expect(
      page.getByRole("button", { name: "Innstillinger" }),
    ).toBeVisible();

    await page.keyboard.press("ControlOrMeta+k");
    await page.getByRole("option", { name: REPORT_TITLE }).click();

    const dialog = page.getByRole("dialog");
    await expect(dialog.getByText(REPORT_TITLE)).toBeVisible();
    // The log tail is shown before anything is sent, verbatim.
    await expect(page.getByTestId("report-log-preview")).toContainText(
      "output child started",
    );
    // Sharing is off, so the one-shot note is on screen BEFORE the button.
    await expect(
      dialog.getByText("engangs-ID", { exact: false }),
    ).toBeVisible();

    await expect(dialog.getByText("0/200")).toBeVisible();
    await dialog.getByLabel("Hva skjedde?").fill("Skjermen ble svart");
    await expect(dialog.getByText("18/200")).toBeVisible();

    await dialog.getByRole("button", { name: "Send rapport" }).click();
    // "Sent", not "queued": with standing consent off this went under a
    // one-shot id, and the dialog says which of the five things happened.
    await expect(
      dialog.getByText("Rapporten er sendt", { exact: false }),
    ).toBeVisible();

    const submitted = (await readCalls(page)).filter(
      (c) => c.cmd === "telemetry_report_submit",
    );
    expect(submitted).toHaveLength(1);
    expect(submitted[0].args.message).toBe("Skjermen ble svart");
    expect(submitted[0].args.logTail).toBe(
      "12:00:01 INFO output child started",
    );
  });
});
