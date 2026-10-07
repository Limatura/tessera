import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://localhost:3001";
const isCI = Boolean(process.env.CI);

// 後端(:8787)要另外先跑起來 —— vite dev 只負責把 /api 代理過去,見 src/tests/e2e/README.md。
// oxlint-disable-next-line import/no-default-export -- Playwright 只認 default export
export default defineConfig({
  forbidOnly: isCI,
  // CI 上不平行、失敗重試兩次;本機相反,要快。
  fullyParallel: !isCI,
  reporter: "html",
  retries: isCI ? 2 : 0,
  testDir: "./src/tests/e2e",
  testMatch: "**/*.spec.ts",
  use: {
    ...devices["Desktop Chrome"],
    baseURL,
    // 用完整 Chromium(new headless),不用預設的 chrome-headless-shell:後者算
    // `container-type: inline-size` 底下的 grid 列高會算成 0,CardHeader 整塊塌掉,
    // 標題、描述全部被判成 hidden。
    channel: "chromium",
    // 斷言寫的是英文字串:cookie 是 paraglide 的第一順位,直接釘住,不靠 Accept-Language 猜。
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
    // 失敗才留 trace/截圖/錄影——成功的 case 留這些只是浪費空間。
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
