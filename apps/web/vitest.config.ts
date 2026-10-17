import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { defineConfig } from "vite-plus";

import { paraglideOptions } from "./vite.config";

// Does not extend vite.config.ts: that loads TanStack Router, React and other
// plugins the unit tests have no use for, and only slows them down. Two pieces
// are taken over, so source files import here exactly as they do in the app:
//
// - `@/` and the other tsconfig `paths`, resolved the same way vite does.
// - Paraglide, which writes `src/paraglide` (gitignored). Without it `@/`
//   resolves `@/paraglide/messages` to a file that does not exist yet on a
//   fresh checkout, and every test importing a translated module fails.
//
// To test components later, add a project with environment: "jsdom".
//
// Coverage is not set here: it is a per-process option that Vitest reads only
// from the root vitest.config.ts, and it is ignored in a project config. The
// rules excluding generated files live there now.
export default defineConfig({
  plugins: [paraglideVitePlugin(paraglideOptions)],
  resolve: {
    tsconfigPaths: true,
  },
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
