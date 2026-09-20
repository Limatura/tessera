import { defineConfig } from "vitest/config";

// 沒有這份的話,在這個目錄底下跑 vitest 會往上找到 root 的設定,然後把那邊的
// `projects` 當成相對於本目錄去解析(`packages/crypto/apps/web`)而爆掉。
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
