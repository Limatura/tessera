import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import react from "ultracite/oxlint/react";
import tanstack from "ultracite/oxlint/tanstack";

export default defineConfig({
  extends: [core, react, tanstack],
  ignorePatterns: [
    ...(core.ignorePatterns ?? []),
    "apps/server/**",
    "apps/docs/**",
    ".agents/**",
    ".claude/**",
    ".codex/**",
    ".roo/**",
    "packages/ui/**",
    "**/*.toml",
  ],
  overrides: [
    {
      files: ["packages/ui/src/components/**", "packages/ui/src/hooks/**"],
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
    // 這條假設 handler 是元件自己定義的 callback（如 handleClick），對「直接
    // 傳外部引用」的情境（如 react-hook-form 的 field.onChange、props.onSave）
    // 系統性地誤判——那些名字是別的 API 定死的，不是我們能重新命名的東西。
    "react/jsx-handler-names": "off",
    // 字母序會打散有意義的欄位分組（例如照敘事順序排列並附解釋註解的
    // config schema），對可讀性沒有實質幫助。
    "eslint/sort-keys": "off",
  },
});
