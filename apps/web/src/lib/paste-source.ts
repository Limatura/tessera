import {
  languageFromClassName,
  languageFromPath,
  normalizeLanguage,
} from "@tessera/upload/languages";

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

/** Every `class` attribute in copied HTML, for the code block's language. */
const CLASS_ATTRIBUTE = /\bclass="(?<value>[^"]*)"/gu;

export function readPasteSource(data: DataTransfer): PasteSource {
  const kind = detectKind(data);

  switch (kind) {
    case "file": {
      const [file] = data.files;
      return {
        filename: file.name,
        kind,
        language: languageFromPath(file.name),
      };
    }
    case "vscode": {
      return { kind, language: readVscodeLanguage(data.getData(VSCODE_MIME)) };
    }
    case "html": {
      return { kind, language: languageInHtml(data.getData("text/html")) };
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

/** The first code-block class in `html` that names a language we know. */
function languageInHtml(html: string): string | null {
  for (const match of html.matchAll(CLASS_ATTRIBUTE)) {
    const language = languageFromClassName(match.groups?.value ?? "");
    if (language) {
      return language;
    }
  }
  return null;
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
