import {
  createBundledHighlighter,
  createSingletonShorthands,
} from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";

// Fine-grained bundle: only the languages this app actually needs, instead of
// shiki's default full bundle (~328 language chunks, ~10MB). See tessera#104 —
// rust-embed bakes the whole `dist/` into the binary, so unused languages are
// dead weight there even though the browser only fetches what it uses.
// 加語言時這裡也要補一筆,否則分享頁會靜默 fallback 成純文字。
// 另外兩處:code-upload-form.tsx 的選單、detect-language.ts 的偵測規則。
const bundledLanguages = {
  c: () => import("@shikijs/langs/c"),
  cpp: () => import("@shikijs/langs/cpp"),
  css: () => import("@shikijs/langs/css"),
  go: () => import("@shikijs/langs/go"),
  html: () => import("@shikijs/langs/html"),
  java: () => import("@shikijs/langs/java"),
  javascript: () => import("@shikijs/langs/javascript"),
  json: () => import("@shikijs/langs/json"),
  markdown: () => import("@shikijs/langs/markdown"),
  php: () => import("@shikijs/langs/php"),
  python: () => import("@shikijs/langs/python"),
  rust: () => import("@shikijs/langs/rust"),
  shellscript: () => import("@shikijs/langs/shellscript"),
  sql: () => import("@shikijs/langs/sql"),
  typescript: () => import("@shikijs/langs/typescript"),
  yaml: () => import("@shikijs/langs/yaml"),
};

const bundledThemes = {
  "dark-plus": () => import("@shikijs/themes/dark-plus"),
  "light-plus": () => import("@shikijs/themes/light-plus"),
};

const createHighlighter = createBundledHighlighter<
  keyof typeof bundledLanguages,
  keyof typeof bundledThemes
>({
  engine: () => createJavaScriptRegexEngine(),
  langs: bundledLanguages,
  themes: bundledThemes,
});

export const { codeToHtml } = createSingletonShorthands(createHighlighter);
