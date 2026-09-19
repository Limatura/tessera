import { defineConfig } from "vitest/config";

// 這裡測的是 lib/ 底下不碰 DOM 的純函式，不需要 jsdom 也不需要任何 plugin。
// 之後要測元件的話，再加一個 environment: "jsdom" 的 project。
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
