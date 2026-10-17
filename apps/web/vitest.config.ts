import { defineConfig } from "vite-plus";

// Does not extend vite.config.ts: that loads paraglide, TanStack Router and other
// plugins, and everything tested here is a pure function that touches neither
// the DOM nor i18n, so loading them only slows things down. To test components
// later, add a project with environment: "jsdom".
//
// Coverage is not set here: it is a per-process option that Vitest reads only
// from the root vitest.config.ts, and it is ignored in a project config. The
// rules excluding generated files live there now.
export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
    environment: "node",
    // Unit tests live next to the file they test (foo.ts / foo.test.ts).
    // Vitest must not load e2e: those use Playwright's test/expect and would
    // fail during collection.
    exclude: ["**/node_modules/**", "src/tests/e2e/**"],
    include: ["src/**/*.test.ts"],
    setupFiles: ["src/tests/unit/crypto-wasm.ts"],
  },
});
