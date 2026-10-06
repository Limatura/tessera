import { paraglideVitePlugin } from "@inlang/paraglide-js";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { visualizer } from "rollup-plugin-visualizer";
import { defineConfig, lazyPlugins } from "vite-plus";

export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
  },
  plugins: lazyPlugins(() => [
    tailwindcss(),
    tanstackRouter({
      autoCodeSplitting: true,
      target: "react",
    }),
    react(),
    paraglideVitePlugin({
      emitTsDeclarations: true,
      outdir: "./src/paraglide",
      project: "./project.inlang",
      // 這個應用沒有 SEO 需求——每條路由要不是應用殼，要不是分享連結
      // 分享連結的整個目的是保持同一個字串，不管誰打開都一樣
      // 如果把語言放在路徑，`/s/ABC` 和 `/en/s/ABC` 就變成同一個分享穿著兩套不同的 URL
      // 這裡什麼好處都撈不著，反而得付出重寫層加上 Rust 那邊再解析一次的代價
      //
      // `cookie` 記得回訪的使用者的選擇；`preferredLanguage`
      // (Accept-Language / navigator.languages) 在首次訪問時挑一個合理的預設
      strategy: ["cookie", "preferredLanguage", "baseLocale"],
    }),
    !!process.env.ANALYZE &&
      visualizer({
        brotliSize: true,
        gzipSize: true,
        open: true,
        template: "list",
      }),
  ]),
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    allowedHosts: [".coder.sao-x.com"],
    port: 3001,
    proxy: {
      "/api": {
        changeOrigin: true,
        target: "http://localhost:8787",
      },
      // 預覽卡片編輯器要用跟 server 畫卡片同一套字型(見 lib/admin/og-renderer.ts)。
      "/fonts": {
        changeOrigin: true,
        target: "http://localhost:8787",
      },
      // `/s/:code` 本身是客戶端路由（分享頁面），但它連結到的內容
      // ——原始文字檢視和檔案下載——直接由後端提供，在 `/api` 之外
      // 只有這兩個路徑後綴才送到後端；裸 `/s/:code` 仍然落到 SPA
      // （在生產環境中沒有開發伺服器：兩者都是同一個嵌入二進位檔，這種分割不存在）
      "^/s/[^/]+/(raw|download)": {
        changeOrigin: true,
        target: "http://localhost:8787",
      },
    },
  },
});
