import {
  createBundledHighlighter,
  createSingletonShorthands,
} from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";

// Fine-grained bundle: only the languages this app needs, instead of shiki's
// default full bundle (~328 language chunks, ~10MB). rust-embed bakes all of
// `dist/` into the binary, so unused languages are dead weight there even
// though the browser only fetches what it uses.
// A language missing here silently falls back to plain text on the share page.
// The other places to update are listed in @tessera/upload's
// `languages/index.ts`.
// svg has no grammar of its own; text-preview.tsx highlights it as xml.
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
  xml: () => import("@shikijs/langs/xml"),
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
