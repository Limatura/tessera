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
    // Written by `scripts/crypto-wasm.sh` and compared against a fresh build in
    // CI. Linting generated glue finds nothing anybody could fix here.
    "packages/crypto/src/wasm/**",
    "**/*.toml",
  ],
  overrides: [
    {
      // 測試檔專用:靜態抓「忘記 await 的非同步斷言」和「整個 it 沒有斷言」。
      // 這兩種寫法在執行時都是綠的,只有 lint 能抓到。
      files: ["**/*.test.ts", "**/*.test.tsx"],
      plugins: ["vitest"],
      rules: {
        "vitest/expect-expect": "error",
        "vitest/no-identical-title": "error",
        "vitest/valid-expect": "error",
      },
    },
    {
      // 「外 function,內 arrow」的機器可檢查部分,只管 web / admin 的 src。
      // 不含各 app 根目錄的 vite.config.ts 等設定檔——那些必須 default export。
      files: ["apps/web/src/**", "apps/admin/src/**"],
      rules: {
        "import/no-default-export": "error",
        // 只管「元件」的定義形式:const Foo = () => <div /> 會被抓,
        // 元件內部的 handler、map callback 不受影響。
        "react/function-component-definition": [
          "error",
          {
            namedComponents: "function-declaration",
            unnamedComponents: "arrow-function",
          },
        ],
        "typescript/no-restricted-types": [
          "error",
          {
            types: {
              "React.FC": {
                message:
                  "不要用 React.FC:泛型寫起來很髒,而且會隱式帶進 children。改成 function Foo({ x }: FooProps)。",
              },
              "React.FunctionComponent": {
                message:
                  "不要用 React.FunctionComponent,改成 function Foo({ x }: FooProps)。",
              },
            },
          },
        ],
      },
    },
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
