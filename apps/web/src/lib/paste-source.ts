/**
 * 從貼上事件的剪貼簿裡讀出「這段東西從哪來、是什麼語言」。
 * 純文字之外的資訊各家寫法不同,能拿到就用,拿不到回 null 交給 detectLanguage 猜。
 */
export type PasteSource =
  /** VS Code 系(Cursor、Windsurf、VSCodium、github.dev 等 Monaco 編輯器)。 */
  | { kind: "vscode"; language: string | null }
  /** 從檔案總管複製檔案再貼上:有真正的檔名。 */
  | { kind: "file"; filename: string; language: string | null }
  /** 網頁上的 code block(GitHub、Prism、highlight.js 渲染的頁面)。 */
  | { kind: "html"; language: string | null }
  | { kind: "plain"; language: null };

/** VS Code 寫在 `vscode-editor-data` 的 JSON(src/vs/editor/browser/controller/editContext/clipboardUtils.ts)。 */
const VSCODE_MIME = "vscode-editor-data";

/** `class="language-ts"`(Prism / highlight.js / markdown-it)或 `highlight-source-ts`(GitHub)。 */
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

/** 判斷順序有意義:檔案最可靠,VS Code 的 metadata 次之,HTML class 只是猜。 */
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
    // 格式是 VS Code 內部的,沒有保證;壞掉就當沒有。
    return null;
  }
}

/**
 * 各家的語言 id / 副檔名 → 我們選單的 value(code-upload-form 的 LANGUAGES)。
 * 加語言時這裡也要加,不然貼上只會退回 detectLanguage 猜。
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

/** 對不到回 null,不要硬塞 plaintext,讓 detectLanguage 有機會猜。 */
export function normalizeLanguage(id: string): string | null {
  return Object.hasOwn(LANGUAGE_ALIASES, id.toLowerCase())
    ? LANGUAGE_ALIASES[id.toLowerCase()]
    : null;
}
