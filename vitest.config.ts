import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "path";

// Vitest runs the React unit + frontend-integration layer. Playwright specs
// under tests/e2e are excluded (they use @playwright/test, not Vitest).
export default defineConfig({
  plugins: [react()],
  resolve: {
    // Vite 8 laster configfiler som ESM, der `__dirname` ikke finnes.
    // `import.meta.dirname` er ESM-ekvivalenten (Node >= 20.11; CI kjører 22).
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./tests/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "tests/integration/**/*.test.{ts,tsx}"],
  },
});
