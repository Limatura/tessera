import { defineConfig } from "vite-plus";

// Without this file, vitest run in this directory finds the root config and
// resolves its `projects` relative to here (`packages/auth/apps/web`), which fails.
export default defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
