/**
 * Recognising a language someone else already named: VS Code's mode, a file
 * extension, or the class a web page put on a code block.
 */

/**
 * Language ids and file extensions as other tools write them (VS Code, file
 * names, code blocks on web pages), mapped to the ids in `LANGUAGE_LABELS`.
 * Add new languages here too, or they fall back to `detectLanguage`'s guess.
 */
const LANGUAGE_ALIASES: Record<string, string> = {
  bash: "shellscript",
  "c++": "cpp",
  c: "cpp",
  cc: "cpp",
  cjs: "javascript",
  cpp: "cpp",
  css: "css",
  cts: "typescript",
  cxx: "cpp",
  go: "go",
  golang: "go",
  h: "cpp",
  hpp: "cpp",
  htm: "html",
  html: "html",
  java: "java",
  javascript: "javascript",
  javascriptreact: "javascript",
  js: "javascript",
  json: "json",
  jsonc: "json",
  jsx: "javascript",
  markdown: "markdown",
  md: "markdown",
  mermaid: "mermaid",
  mjs: "javascript",
  mmd: "mermaid",
  mts: "typescript",
  php: "php",
  py: "python",
  python: "python",
  rs: "rust",
  rust: "rust",
  sh: "shellscript",
  shell: "shellscript",
  shellscript: "shellscript",
  sql: "sql",
  svg: "svg",
  ts: "typescript",
  tsx: "typescript",
  typescript: "typescript",
  typescriptreact: "typescript",
  yaml: "yaml",
  yml: "yaml",
  zsh: "shellscript",
};

/**
 * Unknown ids return null instead of plaintext, so detectLanguage still gets a chance to
 * guess.
 */
export function normalizeLanguage(id: string): string | null {
  const key = id.toLowerCase();
  return Object.hasOwn(LANGUAGE_ALIASES, key)
    ? (LANGUAGE_ALIASES[key] ?? null)
    : null;
}

/**
 * A code block's class names, as Prism, highlight.js and markdown-it write
 * them (`language-ts`, `lang-rs`) or GitHub does (`highlight-source-js`).
 */
const LANGUAGE_CLASS = /\b(?:language|lang|highlight-source)-(?<id>[\w+#-]+)/gu;

/**
 * The language a code block's `class` attribute names, if it names one this
 * project knows. `null` leaves it to `detectLanguage`.
 */
export function languageFromClassName(className: string): string | null {
  for (const match of className.matchAll(LANGUAGE_CLASS)) {
    const language = match.groups?.id
      ? normalizeLanguage(match.groups.id)
      : null;
    if (language) {
      return language;
    }
  }
  return null;
}

const FILE_EXTENSION = /\.(?<ext>[^./\\]+)$/u;

/**
 * The language a file name or path is in, by its extension: `link.rs`,
 * `/blob/main/src/app.tsx`. `null` for no extension or one we do not know.
 */
export function languageFromPath(path: string): string | null {
  const ext = FILE_EXTENSION.exec(path)?.groups?.ext;
  return ext ? normalizeLanguage(ext) : null;
}
