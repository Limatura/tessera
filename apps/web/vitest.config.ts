import { defineConfig } from "vite-plus";

// 刻意不沿用 vite.config.ts:那份會載入 paraglide、TanStack Router 等
// plugin,而這裡測的都是不碰 DOM 也不碰 i18n 的純函式,載進來只是拖慢速度。
// 之後要測元件的話,再加一個 environment: "jsdom" 的 project。
//
// coverage 不寫在這裡:它是「整個 process 一份」的選項,Vitest 只認 root 的
// vitest.config.ts,寫在 project 設定裡會被無視。排除生成檔那幾條規則現在在
// 那邊。
export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
    environment: "node",
    // 單元測試就放在被測檔案旁邊(foo.ts / foo.test.ts)。
    // e2e 由 Playwright 自己跑,vitest 不能碰——它們用的是 Playwright 的
    // test/expect,被 vitest 載到會在收集階段就炸掉。
    exclude: ["**/node_modules/**", "src/tests/e2e/**"],
    include: ["src/**/*.test.ts"],
  },
});
