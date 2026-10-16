// Guesses the language of pasted code. Adding a language means an entry in
// LANGUAGES plus two other places: the select in
// routes/_site/-components/code-upload-form.tsx, and bundledLanguages in
// lib/shiki.ts (highlighting on the share page). `\s` matches newlines; use
// `[ \t]` for spaces only.

const MIN_CHARS_TO_GUESS = 12;
const DEFAULT_MIN_HINTS = 2;

interface LanguageRules {
  /** Must match the option value in code-upload-form.tsx's select. */
  id: string;
  /** Syntax valid only in this language; one match settles it. */
  unique?: readonly RegExp[];
  hints: readonly RegExp[];
  minHints?: number;
}

// On a tie the earlier entry wins, so TypeScript comes before JavaScript and C++ before Java.
const LANGUAGES: readonly LanguageRules[] = [
  {
    // Keywords only mermaid has. First, so `graph TD` is not taken by yaml or markdown.
    id: "mermaid",
    unique: [
      /^[ \t]*(?:graph|flowchart)[ \t]+(?:TB|TD|BT|RL|LR)\b/mu,
      /^[ \t]*(?:sequenceDiagram|classDiagram(?:-v2)?|stateDiagram(?:-v2)?|erDiagram|journey|gantt|pie|quadrantChart|requirementDiagram|gitGraph|mindmap|timeline|sankey-beta|xychart-beta|block-beta|packet-beta|architecture-beta|kanban|radar-beta|treemap-beta)\b/mu,
    ],
    hints: [],
  },
  {
    // Before html: SVG also looks like markup.
    id: "svg",
    unique: [/^\s*(?:<\?xml[^>]*>\s*)?(?:<!--[\s\S]*?-->\s*)*<svg\b/u],
    hints: [],
  },
  {
    id: "rust",
    unique: [/\blet\s+mut\s+/u, /#\[derive\(/u, /\bfn\s+\w+\s*\([^)]*\)\s*->/u],
    hints: [
      /^\s*(?:pub\s+)?fn\s+\w+/mu,
      /\bimpl\s+\w+/u,
      /^\s*use\s+[\w:]+;/mu,
      /->\s*(?:Result|Option|Vec|Self|&?str)\b/u,
      /\b\w+::<?\w/u,
    ],
  },
  {
    id: "go",
    unique: [
      /\bif\s+err\s*!=\s*nil\b/u,
      /^[ \t]*func\s+\(\s*\w+\s+\*?\w+\s*\)\s*\w+\s*\(/mu,
    ],
    hints: [
      /^\s*package\s+\w+\s*$/mu,
      /\bfunc\s+(?:\(\s*\w+\s+\*?\w+\s*\)\s*)?\w+\s*\(/u,
      /:=/u,
      /^\s*import\s+\(/mu,
      /\btype\s+\w+\s+struct\s*\{/u,
    ],
  },
  {
    id: "python",
    unique: [/^[ \t]*def\s+\w+\s*\(.*\)\s*(?:->\s*[\w[\]., |]+\s*)?:\s*$/mu],
    hints: [
      /^\s*(?:from\s+[\w.]+\s+)?import\s+[\w.*]/mu,
      /^\s*class\s+\w+(?:\(.*\))?\s*:/mu,
      /^\s*(?:el)?if\s+.+:\s*$/mu,
      /\bself\b/u,
      // No trailing semicolon, so C-family printf("x") is not taken.
      /^[ \t]*print\s*\(.*\)\s*$/mu,
      /\bprint\s*\(\s*f?["']/u,
      /^[ \t]*@\w+\s*$/mu,
      /\b(?:None|True|False)\b/u,
      /^[ \t]*(?:for|while)\s+.+:\s*$/mu,
      /\b(?:elif|lambda|__init__|__main__)\b/u,
    ],
  },
  {
    id: "php",
    unique: [/<\?php\b/u],
    hints: [/\$\w+\s*=/u, /\becho\s+/u, /->\w+\s*\(/u],
  },
  {
    id: "typescript",
    unique: [
      /^[ \t]*(?:import|export)\s+type\s*[{\w*]/mu,
      /^[ \t]*(?:export\s+)?interface\s+\w+\s*\{/mu,
      /\bsatisfies\s+\w/u,
    ],
    hints: [
      /^\s*(?:export\s+)?type\s+\w+(?:<[^>]*>)?\s*=/mu,
      /:\s*(?:string|number|boolean|void|unknown|any|never)\b/u,
      /\bas\s+const\b/u,
      /^\s*(?:export\s+)?enum\s+\w+/mu,
      /\b(?:public|private|readonly|abstract|implements)\b/u,
      /\b(?:import|export)\s*\{[^}]*\btype\s+\w/u,
      // Type names must be capitalised, so an object literal's key: value does not match.
      /\b(?:const|let|var)\s+\w+\s*:\s*[A-Z]\w*(?:<[^>]*>)?(?:\[\])?\s*=/u,
      /\)\s*:\s*[A-Z]\w*(?:<[^>]*>)?(?:\[\])?\s*\{/u,
      /\b\w+\s*[:?]\s*[A-Z]\w*(?:<[^>]*>)?(?:\[\])?\s*[,;)]/u,
      /\bas\s+[A-Z]\w*\b/u,
      /[?]:\s*\w/u,
    ],
  },
  {
    id: "javascript",
    hints: [
      /\b(?:const|let|var)\s+\w+\s*=/u,
      /\bfunction\s*\w*\s*\(/u,
      /[=]>\s*[{(]/u,
      /\b(?:require\s*\(|module\.exports|export\s+default)\b/u,
      /\bconsole\.(?:log|error|warn)\s*\(/u,
      /^\s*import\s+.*\s+from\s+["']/mu,
    ],
  },
  {
    id: "cpp",
    unique: [/^[ \t]*#include\s*[<"]/mu],
    hints: [
      /\bstd::\w+/u,
      /\b(?:using\s+namespace|template\s*<|nullptr)\b/u,
      /\bint\s+main\s*\(/u,
      /\b(?:cout|cin)\s*<</u,
    ],
  },
  {
    id: "java",
    hints: [
      /\b(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?[\w<>[\]]+\s+\w+\s*\(/u,
      /^\s*package\s+[\w.]+;/mu,
      /^\s*import\s+(?:java|javax)\./mu,
      /\bSystem\.out\.print/u,
      /\bnew\s+[A-Z]\w*\s*\(/u,
      /\b(?:class|interface)\s+[A-Z]\w*/u,
    ],
  },
  {
    id: "sql",
    hints: [
      /\bselect\b[\s\S]*\bfrom\b/iu,
      /\b(?:insert\s+into|update\s+\w+\s+set|delete\s+from)\b/iu,
      /\bcreate\s+(?:table|index|view|database)\b/iu,
      /\b(?:inner|left|right|outer)\s+join\b/iu,
      /\b(?:group\s+by|order\s+by|having)\b/iu,
    ],
  },
  {
    id: "css",
    minHints: 3,
    hints: [
      /^\s*[.#]?[\w-]+(?:\s*[,>+~]\s*[\w.#[\]:-]+)*\s*\{[\s\S]*\}/mu,
      /^\s*[\w-]+\s*:\s*[^;{}]+;/mu,
      /\b\d+(?:px|rem|em|vh|vw|%)\b/u,
      /^\s*@(?:media|import|keyframes|supports)\b/mu,
      /\bvar\(--[\w-]+\)/u,
    ],
  },
  {
    id: "shellscript",
    unique: [/^#!.*\b(?:ba|z|k|da)?sh\b/u],
    hints: [
      /^\s*(?:echo|cd|export|source|sudo|chmod|mkdir|rm|grep|awk|sed|curl)\s+/mu,
      /\$\{?\w+\}?/u,
      /\|\s*(?:grep|awk|sed|xargs|head|tail|sort|jq)\b/u,
      /^\s*(?:if|for|while)\b.*;\s*(?:then|do)\b/mu,
      /\bfi\b|\bdone\b|\besac\b/u,
    ],
  },
  {
    id: "yaml",
    // key: value also matches plain prose, so the bar is higher.
    minHints: 3,
    hints: [
      /^---\s*$/mu,
      /^[\w.-]+:(?:\s+\S|\s*$)/mu,
      /^[ \t]+[\w.-]+:(?:\s+\S|\s*$)/mu,
      /^[ \t]*-\s+[\w"'{[]/mu,
      /^\s*#\s/mu,
    ],
  },
  {
    id: "markdown",
    hints: [
      /^#{1,6}\s+\S/mu,
      /^\s*(?:[*-]|\d+\.)\s+\S/mu,
      /```/u,
      /\[[^\]]+\]\([^)]+\)/u,
      /\*\*\S[\s\S]*?\*\*|__\S[\s\S]*?__/u,
      /^\s*>\s+\S/mu,
    ],
  },
  {
    id: "html",
    unique: [/<!doctype\s+html>/iu],
    hints: [
      /<\/(?:html|head|body|div|span|p|a|ul|li|table|section)>/iu,
      /<(?:div|span|p|a|img|input|br|h[1-6])\b[^>]*>/iu,
      /\b(?:class|id|href|src)\s*=\s*["']/u,
    ],
  },
];

function isJson(code: string): boolean {
  if (!/^[{[]/u.test(code)) {
    return false;
  }
  try {
    const parsed: unknown = JSON.parse(code);
    return typeof parsed === "object" && parsed !== null;
  } catch {
    return false;
  }
}

function countMatches(code: string, patterns: readonly RegExp[]): number {
  let count = 0;
  for (const pattern of patterns) {
    if (pattern.test(code)) {
      count += 1;
    }
  }
  return count;
}

function scoreByHints(code: string): string | null {
  let bestId: string | null = null;
  let bestCount = 0;

  for (const lang of LANGUAGES) {
    const count = countMatches(code, lang.hints);
    const threshold = lang.minHints ?? DEFAULT_MIN_HINTS;
    if (count >= threshold && count > bestCount) {
      bestId = lang.id;
      bestCount = count;
    }
  }

  return bestId;
}

/**
 * Guesses the language of pasted text. Returns null when unsure; the caller
 * should keep the current choice, not fall back to plaintext.
 */
export function detectLanguage(code: string): string | null {
  const trimmed = code.trim();
  if (trimmed.length < MIN_CHARS_TO_GUESS) {
    return null;
  }

  if (isJson(trimmed)) {
    return "json";
  }

  for (const lang of LANGUAGES) {
    if (lang.unique?.some((pattern) => pattern.test(trimmed))) {
      return lang.id;
    }
  }

  return scoreByHints(trimmed);
}
