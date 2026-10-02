import { defineConfig } from "vite-plus";

/*
 * 跨 package 一次跑完,用來拿一份合併的 coverage。
 *
 * 各 package 自己的 vitest.config.ts 仍然是它們的設定來源(include、
 * environment 之類的),這裡只負責把它們聚合起來,外加 coverage —— Vitest 規定
 * coverage、reporters 這些「整個 process 只有一份」的選項必須寫在 root,寫在
 * project 裡會被無視。
 *
 * 這條路跟 `turbo run test` 並存,兩者用途不同:
 *
 * - `pnpm test`(這份)—— 一個 process 跑全部,給得出整體 coverage,沒有快取。
 * - `turbo run test --affected` —— CI 用的,只跑受影響的 package 且吃快取,
 *   但 coverage 是每包各一份。
 *
 * 新增一個有測試的 package 時,要同時加進下面的 projects,否則 `pnpm test`
 * 看不到它(`turbo run test` 會自己從 package.json 的 script 發現,不受影響)。
 */
export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
    coverage: {
      exclude: [
        "**/*.test.ts",
        // 生成檔:paraglide 的 runtime 跟 TanStack Router 的 routeTree 都是
        // build 時才產出的,測它們等於在測別人的 code generator。
        "**/paraglide/**",
        "**/routeTree.gen.ts",
        // e2e 歸 Playwright,不在 vitest 的統計裡。
        "**/src/tests/**",
      ],
      /*
       * 只統計有在跑測試的 package(跟下面的 projects 同一份清單)。
       *
       * 放 `apps/*` / `packages/*` 會把 admin、docs、tui 和整包 shadcn 元件
       * 都算進分母 —— 那些現在一個測試都沒有,分母灌滿「從來沒打算用 vitest
       * 測的東西」之後,整體數字會永遠停在個位數,看不出任何趨勢。
       *
       * 只收 `.ts` 不收 `.tsx`:元件要 jsdom 才測得動,現在一個都沒測,
       * 收進來一樣是灌分母。之後加了元件測試,再把 `.tsx` 和 ui 的
       * components 一起放回來。
       */
      include: [
        "apps/web/src/**/*.ts",
        "packages/crypto/src/**/*.ts",
        "packages/i18n/src/**/*.ts",
        "packages/ui/src/lib/**/*.ts",
      ],
      provider: "v8",
      reporter: ["text", "lcov"],
    },
    projects: ["apps/web", "packages/crypto", "packages/i18n", "packages/ui"],
  },
});
