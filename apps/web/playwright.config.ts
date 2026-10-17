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
  // Every test makes and deletes its own shares and signs in with its own
  // session, so they can run side by side in CI too.
  fullyParallel: true,
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
    command: "pnpm dev",
    reuseExistingServer: !isCI,
    url: baseURL,
  },
  // A hosted runner has 4 vCPUs, shared with the backend and vite dev; three
  // browsers leave them a core. Serial took the suite to over two minutes.
  workers: isCI ? 3 : "50%",
});
