import { defineConfig } from "vitest/config";

// 刻意不沿用 vite.config.ts:那份會載入 paraglide、TanStack Router 等
// plugin,而這裡測的都是不碰 DOM 也不碰 i18n 的純函式,載進來只是拖慢速度。
// 之後要測元件的話,再加一個 environment: "jsdom" 的 project。
export default defineConfig({
  test: {
    coverage: {
      exclude: [
        "**/*.test.ts",
        "src/paraglide/**",
        "src/routeTree.gen.ts",
        "src/tests/**",
      ],
      include: ["src/**/*.{ts,tsx}"],
      provider: "v8",
      reporter: ["text", "lcov"],
    },
    environment: "node",
    // 單元測試就放在被測檔案旁邊(foo.ts / foo.test.ts)。
    // e2e 由 Playwright 自己跑,vitest 不能碰——它們用的是 Playwright 的
    // test/expect,被 vitest 載到會在收集階段就炸掉。
    exclude: ["**/node_modules/**", "src/tests/e2e/**"],
    include: ["src/**/*.test.ts"],
  },
});
