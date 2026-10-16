import { defineConfig } from "vite-plus";

/*
 * Runs every package's tests in one go, to get one combined coverage report.
 *
 * Each package's own vitest.config.ts is still where its settings come from
 * (include, environment and so on); this only gathers them and adds coverage.
 * Vitest requires options there is one of per process, such as coverage and
 * reporters, to be set at the root: in a project they are ignored.
 *
 * This sits beside `turbo run test`, and the two do different jobs:
 *
 * - `pnpm test` (this file): one process runs everything, gives overall
 *   coverage, no caching.
 * - `turbo run test --affected`: what CI runs. Only the affected packages, with
 *   caching, but one coverage report per package.
 *
 * A new package with tests has to be added to `projects` below too, or
 * `pnpm test` will not see it (`turbo run test` finds it from the package.json
 * script by itself).
 */
export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
    coverage: {
      exclude: [
        "**/*.test.ts",
        // Generated: paraglide's runtime and TanStack Router's routeTree are
        // produced at build time, and testing them tests someone else's generator.
        "**/paraglide/**",
        "**/routeTree.gen.ts",
        // e2e belongs to Playwright, not to vitest's numbers.
        "**/src/tests/**",
      ],
      /*
       * Only the packages that have tests (the same list as `projects` below).
       *
       * `apps/*` and `packages/*` would put admin, docs, tui and every shadcn
       * component in the denominator. None of those has a single test, and a
       * denominator full of things nobody meant to test with vitest keeps the
       * total in single digits for ever, showing no trend at all.
       *
       * `.ts` only, not `.tsx`: components need jsdom to test, none are tested
       * yet, and including them would pad the denominator the same way. Put
       * `.tsx` and ui's components back once component tests exist.
       */
      include: [
        "apps/web/src/**/*.ts",
        "packages/crypto/src/**/*.ts",
        "packages/ui/src/lib/**/*.ts",
      ],
      provider: "v8",
      reporter: ["text", "lcov"],
    },
    projects: ["apps/web", "packages/crypto", "packages/ui"],
  },
});
