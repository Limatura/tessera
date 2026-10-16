/**
 * Reads where a paste came from and what language it is, from the clipboard of
 * a paste event. Anything beyond plain text differs by source; it is used when
 * present, otherwise null leaves the guess to detectLanguage.
 */
export type PasteSource =
  /**
   * VS Code and its forks (Cursor, Windsurf, VSCodium, github.dev and other Monaco
   * editors).
   */
  | { kind: "vscode"; language: string | null }
  /** A file copied in the file manager and pasted: it has a real file name. */
  | { kind: "file"; filename: string; language: string | null }
  /** A code block on a web page (GitHub, or pages rendered by Prism or highlight.js). */
  | { kind: "html"; language: string | null }
  | { kind: "plain"; language: null };

/**
 * JSON VS Code writes to `vscode-editor-data`
 * (src/vs/editor/browser/controller/editContext/clipboardUtils.ts).
 */
const VSCODE_MIME = "vscode-editor-data";

/**
 * `class="language-ts"` (Prism, highlight.js, markdown-it) or `highlight-source-ts`
 * (GitHub).
 */
const HTML_LANGUAGE_CLASS =
  /class="[^"]*\b(?:language|lang|highlight-source)-(?<id>[\w+#-]+)/u;

const FILE_EXTENSION = /\.(?<ext>[^./\\]+)$/u;

export function readPasteSource(data: DataTransfer): PasteSource {
  const kind = detectKind(data);

  switch (kind) {
    case "file": {
      const [file] = data.files;
      const ext = FILE_EXTENSION.exec(file.name)?.groups?.ext;
      return {
        filename: file.name,
        kind,
        language: ext ? normalizeLanguage(ext) : null,
      };
    }
    case "vscode": {
      return { kind, language: readVscodeLanguage(data.getData(VSCODE_MIME)) };
    }
    case "html": {
      const id = HTML_LANGUAGE_CLASS.exec(data.getData("text/html"))?.groups
        ?.id;
      return { kind, language: id ? normalizeLanguage(id) : null };
    }
    default: {
      return { kind: "plain", language: null };
    }
  }
}

/**
 * Order matters: a file is the most reliable, VS Code metadata next, an HTML class is
 * only a guess.
 */
function detectKind(data: DataTransfer): PasteSource["kind"] {
  const types = new Set(data.types);
  if (data.files.length > 0) {
    return "file";
  }
  if (types.has(VSCODE_MIME)) {
    return "vscode";
  }
  if (types.has("text/html")) {
    return "html";
  }
  return "plain";
}

function readVscodeLanguage(raw: string): string | null {
  try {
    const { mode } = JSON.parse(raw) as { mode?: unknown };
    return typeof mode === "string" ? normalizeLanguage(mode) : null;
  } catch {
    // The format is internal to VS Code and not guaranteed; if it breaks, treat it as
    // absent.
    return null;
  }
}

/**
 * Language ids and file extensions from each source, mapped to our select
 * values (LANGUAGES in code-upload-form). Add new languages here too, or a
 * paste falls back to detectLanguage's guess.
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
  return Object.hasOwn(LANGUAGE_ALIASES, id.toLowerCase())
    ? LANGUAGE_ALIASES[id.toLowerCase()]
    : null;
}
