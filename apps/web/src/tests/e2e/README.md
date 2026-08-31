# End-to-end 測試

這個目錄是**先空出來的**——還沒裝 Playwright,也還沒有測試。要開始寫的時候照下面做。寫法參考了 [nelsonlaidev/nelsonlai.dev][ref],但這個專案是 Rust 後端＋ SQLite,有幾處不能照抄,下面會標出來。

[ref]: https://github.com/nelsonlaidev/nelsonlai.dev

## 為什麼是 Playwright

原生 Playwright,不是 Vitest Browser Mode。e2e 要跑的是「上傳 → 拿到分享碼 → 打開連結 → 看到內容」這種整條流程,Playwright 的 auto-wait、trace viewer 和多瀏覽器支援在這種情境成熟得多;Browser Mode 偏元件層級。

## 為什麼跟單元測試分開放

單元測試是 colocated 的(`lib/upload.ts` 旁邊就是 `lib/upload.test.ts`),因為改實作時測試就在隔壁。e2e 不行:

- 它們用的是 Playwright 的 `test` / `expect`,不是 vitest 的。vitest 只要載到就會在收集階段炸掉,所以 `vitest.config.ts` 明確 exclude 了這個目錄。
- e2e 需要跑起來的伺服器和瀏覽器,跟純函式測試的執行條件完全不同。

**副檔名用 `.spec.ts`**(不是 `.test.ts`),多一層保險:就算哪天 exclude 被改壞了,`.spec.ts` 也不在 vitest 的 include 裡。

## 開始寫的時候

```bash
pnpm --filter web add -D @playwright/test
pnpm --filter web exec playwright install --with-deps chromium
```

### `apps/web/playwright.config.ts`

```ts
import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://localhost:3001";
const isCI = !!process.env.CI;

export default defineConfig({
  testDir: "./src/tests/e2e",
  testMatch: "**/*.spec.ts",
  // CI 上不平行、失敗重試兩次;本機相反,要快。
  fullyParallel: !isCI,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  workers: isCI ? 1 : "50%",
  reporter: "html",
  use: {
    baseURL,
    // 失敗才留 trace/截圖/錄影——成功的 case 留這些只是浪費 CI 空間。
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    ...devices["Desktop Chrome"],
    // 分享頁有「複製連結」,不給權限那條路測不了。
    contextOptions: { permissions: ["clipboard-read", "clipboard-write"] },
  },
  webServer: {
    // vite dev 會把 /api 代理到 :8787,所以後端要另外先跑起來。
    command: "pnpm dev",
    url: baseURL,
    reuseExistingServer: !isCI,
  },
});
```

### 這個專案跟參考 repo 不一樣的地方

**後端起不起得來是主要差異。** 參考 repo 是 Next.js 單體,`webServer` 一行就全起來了。這裡前後端是分開的:vite dev(:3001)把 `/api` 代理到 Rust 伺服器 (:8787),所以 e2e 跑之前要先有後端。兩種做法:

- `pnpm docker:up`(整包 compose,前端也在裡面,直接測 :8787 就好,不用 vite)
- 或本機 `pnpm server:dev` 再讓 Playwright 的 `webServer` 起 vite

**不要照抄他們的 auth setup。** 他們用 drizzle 直接寫一筆 session 進 DB、再自己簽出 cookie,那是因為前後端同一個 JS 專案、共用 DB client。這裡的 DB 歸 Rust 管,前端沒有(也不該有)直接寫入的路徑。要測登入狀態的話走 API:打 `POST /api/v1/auth/login` 拿 cookie,存成 `storageState`。

**測試資料要自己收乾淨。** 他們有 `global.teardown.ts` 把測試產生的資料刪掉。這裡的分享大多會自己過期,但 e2e 建的分享還是該在 teardown 用 `DELETE /api/v1/shares/{code}`(帶 management token)刪掉,不然 `./data` 會一直長。

**要不要分 authenticated / unauthenticated 兩個 project?** 他們用 `projects` + `dependencies` 把登入狀態準備好一次給多個測試共用。這裡大部分功能 (上傳、分享、瀏覽)匿名就能用,所以先寫成單一 project 就好;等真的有多個需要登入的測試再拆。

### 第一個該寫的測試

冒煙測試,涵蓋這個產品的主要路徑:

1. 開首頁 → 貼一段程式碼 → 送出
2. 拿到分享連結
3. 用新的 page 打開那個連結 → 看得到剛才的內容
4. teardown 把這筆分享刪掉

`data-testid` 目前程式碼裡幾乎沒有,寫的時候會需要補一些。優先用 `getByRole` / `getByLabel` 這類語意查詢,查不到再加 testid。

### 還要補的三個地方

script 名稱已經留好空間了(`test:unit*`),照這個模式補:

```jsonc
"test:e2e": "playwright test",
"test:e2e:ui": "playwright test --ui",
```

- `turbo.json` 加一個 `test:e2e` task(`"cache": false`,因為會碰真的伺服器)
- `.github/workflows/web.yml` 加一個 job:裝 Playwright(`--with-deps chromium --only-shell`)、起後端、跑測試,失敗時用 `actions/upload-artifact` 把 `playwright-report/` 存起來(`if: always()`)

> 本機注意:這台機器的 Playwright chromium 缺 `libnspr4` 等系統函式庫, 要先跑 `playwright install --with-deps`。CI 的 runner 沒這問題。
