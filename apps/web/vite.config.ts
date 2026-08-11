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
      // This app has no SEO surface — every route is either the app shell or a
      // share link, and a share link's whole point is staying the same string
      // regardless of who opens it. Putting the locale in the path would mean
      // `/s/ABC` and `/en/s/ABC` are "the same" share wearing two different
      // URLs, which buys nothing here and costs a rewrite layer plus a second
      // parser on the Rust side.
      //
      // `cookie` remembers a returning visitor's choice; `preferredLanguage`
      // (Accept-Language) picks a sensible default on the first visit.
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
