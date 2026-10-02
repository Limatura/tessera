import { defineConfig } from "vite-plus";

// 這裡測的是 lib/ 底下不碰 DOM 的純函式，不需要 jsdom 也不需要任何 plugin。
// 之後要測元件的話，再加一個 environment: "jsdom" 的 project。
export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
