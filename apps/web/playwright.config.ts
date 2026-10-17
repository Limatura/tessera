import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://localhost:3001";
const isCI = Boolean(process.env.CI);

/** Where the setup project leaves the signed-in admin's cookies. */
export const ADMIN_STORAGE_STATE = "playwright/.auth/admin.json";
/** Tests tagged with this run signed in, in the `authenticated` project. */
const AUTHENTICATED_TAG = /@authenticated/u;

// The backend (:12923) must already be running; vite dev only proxies /api to it.
// See src/tests/e2e/README.md.
// oxlint-disable-next-line import/no-default-export -- Playwright only reads the default export
export default defineConfig({
  forbidOnly: isCI,
  // CI runs serially: password key derivation is CPU-bound by design, and on
  // a 4-vCPU runner parallel browsers starved each other into timeouts.
  fullyParallel: !isCI,
  // CI logs show which test is running and which failed; the html report is
  // uploaded as an artifact.
  reporter: isCI ? [["list"], ["html", { open: "never" }]] : "html",
  retries: isCI ? 2 : 0,
  testDir: "./src/tests/e2e",
  // `setup` signs in once through the API and saves the cookies; the
  // `authenticated` project reuses them for tests tagged @authenticated, and
  // `anonymous` runs everything else with no session at all.
  projects: [
    { name: "setup", testMatch: "**/*.setup.ts" },
    {
      dependencies: ["setup"],
      grep: AUTHENTICATED_TAG,
      name: "authenticated",
      testMatch: "**/*.spec.ts",
      use: { storageState: ADMIN_STORAGE_STATE },
    },
    {
      grepInvert: AUTHENTICATED_TAG,
      name: "anonymous",
      testMatch: "**/*.spec.ts",
    },
  ],
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
    // CI serves a production build: vite dev compiles each module on first
    // request, which a fresh runner pays on every page. The build takes
    // seconds, and preview proxies /api the same way dev does.
    command: isCI
      ? "pnpm build && pnpm exec vp preview --port 3001 --strictPort"
      : "pnpm dev",
    reuseExistingServer: !isCI,
    // The default 60 seconds covers starting vite dev, not a production build
    // first on a slow runner, and a timeout here fails before any test runs.
    timeout: 120_000,
    url: baseURL,
  },
  workers: isCI ? 1 : "50%",
});
