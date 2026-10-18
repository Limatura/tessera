import { defineConfig } from "@playwright/test";

const isCI = Boolean(process.env.CI);

// The extension is loaded into a persistent Chromium profile per test (see
// `tests/e2e/fixtures.ts`), so browser settings live there, not under `use`.
// One worker: the tests share one throwaway server, and some of them switch
// encryption on or off for the whole instance.
export default defineConfig({
  forbidOnly: isCI,
  fullyParallel: false,
  globalSetup: "./tests/e2e/global-setup.ts",
  reporter: isCI ? [["list"], ["html", { open: "never" }]] : "list",
  retries: isCI ? 2 : 0,
  testDir: "./tests/e2e",
  testMatch: "**/*.spec.ts",
  timeout: 60_000,
  use: {
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  workers: 1,
});
