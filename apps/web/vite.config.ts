import { paraglideVitePlugin } from "@inlang/paraglide-js";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { visualizer } from "rollup-plugin-visualizer";
import { defineConfig, lazyPlugins } from "vite-plus";

// Which backend the dev server proxies the API to. Defaults to `pnpm server:dev` on :12923.
// Set this to point e2e at a fresh throwaway backend without stopping the one you are using.
const apiOrigin = process.env.TESSERA_API_ORIGIN ?? "http://localhost:12923";

// Shared with vitest.config.ts, which compiles the same messages for unit tests.
export const paraglideOptions = {
  emitTsDeclarations: true,
  outdir: "./src/paraglide",
  project: "./project.inlang",
  // No SEO needs here: every route is either the app shell or a share link,
  // and a share link must stay the same string for everyone who opens it.
  // A locale in the path would give one share two URLs, `/s/ABC` and
  // `/en/s/ABC`, gaining nothing and costing a rewrite layer plus a second
  // parse on the Rust side.
  //
  // `cookie` remembers a returning visitor's choice; `preferredLanguage`
  // (Accept-Language / navigator.languages) picks a sensible first default.
  // `custom-instance` is the operator's `site.language`, which the server
  // writes into index.html (see lib/locale.ts); `baseLocale` only answers
  // where there is no server, such as the dev server.
  strategy: ["cookie", "preferredLanguage", "custom-instance", "baseLocale"],
} satisfies Parameters<typeof paraglideVitePlugin>[0];

export default defineConfig({
  build: {
    // The 500 kB default misfires here: cpp.js, mermaid's per-diagram chunks,
    // cytoscape and the like load only when a user needs that grammar or
    // diagram, so they do not affect first paint. The entry chunk (index.js) is
    // about 117 kB gzipped. Raising the limit keeps the check meaningful: a large
    // dependency pulled into the main bundle by mistake still trips it.
    chunkSizeWarningLimit: 800,
  },
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
    paraglideVitePlugin(paraglideOptions),
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
        target: apiOrigin,
      },
      // The card preview editor uses the same fonts the server renders cards
      // with (see lib/admin/og-renderer.ts).
      "/fonts": {
        changeOrigin: true,
        target: apiOrigin,
      },
      // Generated per instance (name, colours), see apps/server/src/manifest.rs.
      "/manifest.webmanifest": {
        changeOrigin: true,
        target: apiOrigin,
      },
      // `/s/:code` itself is a client route (the share page), but what it links
      // to, the raw text view, file download and split-file pieces, is served by the backend
      // outside `/api`. Only these suffixes go to the backend; bare
      // `/s/:code` still falls through to the SPA. (In production there is no
      // dev server: both are the same embedded binary and this split does not
      // exist.)
      "^/s/[^/]+/(raw|download|segments)": {
        changeOrigin: true,
        target: apiOrigin,
      },
    },
  },
});
