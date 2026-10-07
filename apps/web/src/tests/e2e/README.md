# End-to-end 測試

首頁冒煙測試在 `upload.spec.ts`,設定在 `apps/web/playwright.config.ts`。寫法參考了 [nelsonlaidev/nelsonlai.dev][ref],但這個專案是 Rust 後端＋ SQLite,有幾處不能照抄,下面會標出來。

```bash
pnpm server:dev                    # 先起後端(:8787)
pnpm --filter web test:e2e         # Playwright 會自己起 vite dev(:3001),已經在跑就沿用
```

不想停掉手上那台後端的話,另外起一台拋棄式的,用 `TESSERA_API_ORIGIN` 把 vite 的代理指過去:

```bash
TESSERA_SERVER__PORT=8788 TESSERA_SERVER__DATA_DIR=/tmp/tessera-e2e \
  TESSERA_DATABASE__URL=sqlite:///tmp/tessera-e2e/tessera.db target/debug/tessera-server &
TESSERA_API_ORIGIN=http://127.0.0.1:8788 pnpm --filter web test:e2e
```

CI 是 `.github/workflows/web.yml` 的 `e2e` job:`web` 被動到才跑,每次用空的資料目錄起一台後端,跑 `pnpm turbo run test:e2e --filter=web`,不管成敗都把 `playwright-report/`(含後端 log)上傳成 artifact。

[ref]: https://github.com/nelsonlaidev/nelsonlai.dev

## 為什麼是 Playwright

原生 Playwright,不是 Vitest Browser Mode。e2e 要跑的是「上傳 → 拿到分享碼 → 打開連結 → 看到內容」這種整條流程,Playwright 的 auto-wait、trace viewer 和多瀏覽器支援在這種情境成熟得多;Browser Mode 偏元件層級。

## 為什麼跟單元測試分開放

單元測試是 colocated 的(`lib/upload.ts` 旁邊就是 `lib/upload.test.ts`),因為改實作時測試就在隔壁。e2e 不行:

- 它們用的是 Playwright 的 `test` / `expect`,不是 vitest 的。vitest 只要載到就會在收集階段炸掉,所以 `vitest.config.ts` 明確 exclude 了這個目錄。
- e2e 需要跑起來的伺服器和瀏覽器,跟純函式測試的執行條件完全不同。

**副檔名用 `.spec.ts`**(不是 `.test.ts`),多一層保險:就算哪天 exclude 被改壞了,`.spec.ts` 也不在 vitest 的 include 裡。

## 現有的測試

`upload.spec.ts`,全部匿名:

- 首頁在碰到程式碼分頁之前不載入 CodeMirror —— 守 `upload-panel.tsx` 的 lazy 載入,有人把 `CodeUploadForm` 改回靜態 import 就會紅
- 偵測到語言後編輯器會載入語法包並上色 —— 守 `code-upload-form.tsx` 的語法包動態載入
- 上傳程式碼片段 → 取件碼 → 另一個分頁取回 → 刪除
- 上傳檔案 → 取件碼 → 另一個分頁取回 → 刪除

斷言用英文字串,語系由 `playwright.config.ts` 用 `PARAGLIDE_LOCALE` cookie 釘成 `en`。優先用 `getByRole` / `getByLabel` 這類語意查詢,查不到再加 `data-testid`。

## 這個專案跟參考 repo 不一樣的地方

**後端起不起得來是主要差異。** 參考 repo 是 Next.js 單體,`webServer` 一行就全起來了。這裡前後端是分開的:vite dev(:3001)把 `/api` 代理到 Rust 伺服器 (:8787),所以 e2e 跑之前要先有後端。兩種做法:

- `pnpm docker:up`(整包 compose,前端也在裡面,直接測 :8787 就好,不用 vite)
- 或本機 `pnpm server:dev` 再讓 Playwright 的 `webServer` 起 vite

**不要照抄他們的 auth setup。** 他們用 drizzle 直接寫一筆 session 進 DB、再自己簽出 cookie,那是因為前後端同一個 JS 專案、共用 DB client。這裡的 DB 歸 Rust 管,前端沒有(也不該有)直接寫入的路徑。要測登入狀態的話走 API:打 `POST /api/v1/auth/login` 拿 cookie,存成 `storageState`。

**測試資料要自己收乾淨。** 他們有 `global.teardown.ts` 把測試產生的資料刪掉。這裡目前是每個上傳測試最後按結果畫面上的「Delete」—— management token 只在那個畫面上,順便也測到刪除。代價是測試中途失敗的那筆不會被刪,只能等它自己過期(預設 7 天);之後要更乾淨的話,改成從上傳的回應攔下 token,在 `afterEach` 打 `DELETE /api/v1/shares/{code}`。

**要不要分 authenticated / unauthenticated 兩個 project?** 他們用 `projects` + `dependencies` 把登入狀態準備好一次給多個測試共用。這裡大部分功能 (上傳、分享、瀏覽)匿名就能用,所以先寫成單一 project 就好;等真的有多個需要登入的測試再拆。

> 注意:Playwright 的 chromium 需要 `libnspr4`、`libnss3`、`libcups2` 等系統函式庫,缺的話先跑 `playwright install --with-deps chromium`;那份清單沒涵蓋到的發行版(例如 Ubuntu 26.04)就用 `ldd ~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome | grep "not found"` 看缺什麼,自己 apt 裝。CI 的 job 不跑 `--with-deps`(那要 sudo),缺函式庫時會直接列出來並失敗。
>
> 設定裡的 `channel: "chromium"` 不要拿掉:預設的 chrome-headless-shell 會把 `container-type: inline-size` 底下的 grid 列高算成 0,`CardHeader` 整塊塌掉,所有卡片標題都被判成 hidden。CI 裝的時候也就不能用 `--only-shell`。
