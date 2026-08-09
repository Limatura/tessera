import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import react from "ultracite/oxlint/react";
import tanstack from "ultracite/oxlint/tanstack";

export default defineConfig({
  extends: [core, react, tanstack],
  ignorePatterns: core.ignorePatterns,

  rules: {
    "func-style": "off",
    "no-use-before-define": "off",
    "react/no-unescaped-entities": "off",
  },
});
