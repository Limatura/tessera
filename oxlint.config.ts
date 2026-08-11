import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import react from "ultracite/oxlint/react";
import tanstack from "ultracite/oxlint/tanstack";

export default defineConfig({
  extends: [core, react, tanstack],
  ignorePatterns: [
    ...(core.ignorePatterns ?? []),
    "apps/server/**",
    ".agents/**",
    ".claude/**",
    ".codex/**",
  ],
  overrides: [
    {
      files: ["packages/ui/src/components/**"],
      rules: {
        "eslint/complexity": "off",
        "eslint/no-param-reassign": "off",
        "eslint/no-shadow": "off",
        "jsx-a11y/label-has-associated-control": "off",
        "jsx-a11y/prefer-tag-over-role": "off",
        "react/button-has-type": "off",
        "react/hook-use-state": "off",
        "react/jsx-no-constructed-context-values": "off",
        "react/no-danger": "off",
        "react/no-object-type-as-default-prop": "off",
        "react/react-compiler": "off",
        "unicorn/no-document-cookie": "off",
      },
    },
  ],
  rules: {
    "func-style": "off",
    "no-use-before-define": "off",
    "react/no-unescaped-entities": "off",
  },
});
