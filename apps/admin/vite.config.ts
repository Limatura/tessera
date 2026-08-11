import { paraglideVitePlugin } from "@inlang/paraglide-js";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
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
    port: 3001,
  },
});
