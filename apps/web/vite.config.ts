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
      // The locale is the URL. `localStorage` used to be first here, which put
      // it somewhere the server cannot read — and the server is what injects a
      // share's link preview, so an unfurled link had no way to know which
      // language to render.
      //
      // Nothing follows `url` except the fallback: the default URL pattern maps
      // the base locale to the unprefixed path, so `url` matches every address
      // and anything after it would be unreachable.
      strategy: ["url", "baseLocale"],
    }),
  ],
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    port: 3001,
  },
});
