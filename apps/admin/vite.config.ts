import { paraglideVitePlugin } from "@inlang/paraglide-js";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig(({ command }) => ({
  // 正式 build 由 apps/server 嵌進 binary、掛在 `/admin/` 底下，資源路徑要帶這個
  // 前綴；dev server 還是在根路徑，開發流程不變。
  base: command === "build" ? "/admin/" : "/",
  plugins: [
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
      // 跟 apps/web 保持一致：cookie 记住选择，preferredLanguage 读
      // Accept-Language 做首次访问的默认值。
      strategy: ["cookie", "preferredLanguage", "baseLocale"],
    }),
  ],
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    allowedHosts: [".coder.sao-x.com"],
    port: 3002,
    proxy: {
      // dev 才需要：admin 跟後端是兩個 process，`@tessera/api` 的 baseUrl 是 `/`，
      // 不轉過去的話 `/api/...` 會落到 SPA fallback、拿回 index.html 而不是 JSON。
      // 正式環境同網域由反向代理處理，沒有這一層。
      "/api": {
        changeOrigin: true,
        target: "http://localhost:8787",
      },
    },
  },
}));
