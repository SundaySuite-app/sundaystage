import { test, expect } from "@playwright/test";

// Smoke: the built frontend boots in a plain browser (no Tauri runtime) and
// the app shell renders. IPC calls reject without Tauri, but the shell — and
// the brand — must still paint.
test("app shell renders", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("SundayStage").first()).toBeVisible();
});
