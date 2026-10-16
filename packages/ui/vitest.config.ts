import { defineConfig } from "vite-plus";

// Tests pure functions under lib/ that never touch the DOM: no jsdom, no plugins.
// To test components later, add a project with environment: "jsdom".
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
