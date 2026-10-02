import { defineConfig } from "vite-plus";

// 沒有這份的話,在這個目錄底下跑 vitest 會往上找到 root 的設定,然後把那邊的
// `projects` 當成相對於本目錄去解析(`packages/crypto/apps/web`)而爆掉。
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
