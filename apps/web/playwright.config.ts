import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://localhost:3001";
const isCI = Boolean(process.env.CI);

// The backend (:12923) must already be running; vite dev only proxies /api to it.
// See src/tests/e2e/README.md.
// oxlint-disable-next-line import/no-default-export -- Playwright only reads the default export
export default defineConfig({
  forbidOnly: isCI,
  // CI runs serially and retries twice; locally the opposite, for speed.
  fullyParallel: !isCI,
  // CI logs show which test is running and which failed; the html report is
  // uploaded as an artifact.
  reporter: isCI ? [["list"], ["html", { open: "never" }]] : "html",
  retries: isCI ? 2 : 0,
  testDir: "./src/tests/e2e",
  testMatch: "**/*.spec.ts",
  use: {
    ...devices["Desktop Chrome"],
    baseURL,
    // Full Chromium (new headless), not the default chrome-headless-shell: the
    // latter computes grid row heights under `container-type: inline-size` as 0,
    // collapsing CardHeader so its title and description count as hidden.
    channel: "chromium",
    // Assertions use English strings. The cookie is paraglide's first choice,
    // so pin it instead of relying on Accept-Language.
    locale: "en-US",
    screenshot: "only-on-failure",
    storageState: {
      cookies: [
        {
          domain: "localhost",
          expires: -1,
          httpOnly: false,
          name: "PARAGLIDE_LOCALE",
          path: "/",
          sameSite: "Lax",
          secure: false,
          value: "en",
        },
      ],
      origins: [],
    },
    // Keep traces, screenshots and video only for failures; passing runs would just waste space.
    trace: "retain-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: "pnpm dev",
    reuseExistingServer: !isCI,
    url: baseURL,
  },
  workers: isCI ? 1 : "50%",
});
