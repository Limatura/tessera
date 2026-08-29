// 貼上程式碼時猜語言(tessera#228)。
// 刻意做得簡單:猜錯的成本只是使用者改一下選單,猜對就省一個步驟。
// 只涵蓋上傳表單選單裡有的語言。

const MIN_CHARS_TO_GUESS = 12;

interface Rule {
  language: string;
  patterns: readonly RegExp[];
}

// 有順序:分數相同時排前面的贏,所以把容易被誤判的語言放前面
// (TypeScript 在 JavaScript 前、C++ 在 Java 前)。
const RULES: readonly Rule[] = [
  {
    language: "rust",
    patterns: [
      /^\s*(?:pub\s+)?fn\s+\w+/mu,
      /\blet\s+mut\s+/u,
      /\bimpl\s+\w+/u,
      /^\s*use\s+[\w:]+;/mu,
      /->\s*(?:Result|Option|Vec|Self|&?str)\b/u,
      /\b\w+::<?\w/u,
      /#\[derive\(/u,
    ],
  },
  {
    language: "go",
    patterns: [
      /^\s*package\s+\w+\s*$/mu,
      /\bfunc\s+(?:\(\s*\w+\s+\*?\w+\s*\)\s*)?\w+\s*\(/u,
      /:=/u,
      /^\s*import\s+\(/mu,
      /\bif\s+err\s*!=\s*nil\b/u,
      /\btype\s+\w+\s+struct\s*\{/u,
    ],
  },
  {
    language: "python",
    patterns: [
      /^\s*def\s+\w+\s*\(.*\)\s*(?:->\s*[\w[\]., |]+\s*)?:/mu,
      /^\s*(?:from\s+[\w.]+\s+)?import\s+[\w.*]/mu,
      /^\s*class\s+\w+(?:\(.*\))?\s*:/mu,
      /^\s*(?:el)?if\s+.+:\s*$/mu,
      /\bself\b/u,
      // 沒有任何宣告的 Python 片段(例如只有 print)。
      // 綁定行首且行尾不能有分號,才不會搶走 C 家族的 printf("x");
      /^[ \t]*print\s*\(.*\)\s*$/mu,
      /\bprint\s*\(\s*f?["']/u,
      /^[ \t]*@\w+\s*$/mu,
      /\b(?:None|True|False)\b/u,
      /^[ \t]*(?:for|while)\s+.+:\s*$/mu,
      /\b(?:elif|lambda|__init__|__main__)\b/u,
    ],
  },
  {
    language: "php",
    patterns: [/<\?php\b/u, /\$\w+\s*=/u, /\becho\s+/u, /->\w+\s*\(/u],
  },
  {
    language: "typescript",
    patterns: [
      /^\s*(?:export\s+)?interface\s+\w+\s*\{/mu,
      /^\s*(?:export\s+)?type\s+\w+(?:<[^>]*>)?\s*=/mu,
      /:\s*(?:string|number|boolean|void|unknown|any|never)\b/u,
      /\b(?:as\s+const|satisfies)\b/u,
      /^\s*(?:export\s+)?enum\s+\w+/mu,
      /\b(?:public|private|readonly|abstract|implements)\b/u,
      // import type / export type 只有 TS 有,
      // 用自訂型別而非原生型別標註的專案很常見。
      /^[ \t]*(?:import|export)\s+type\s*[{\w*]/mu,
      // export { type Foo } 這種寫在大括號裡的型別。
      /\b(?:import|export)\s*\{[^}]*\btype\s+\w/u,
      // 用自訂型別標註的宣告,例如 const x: Foo[] =。
      // 型別名要大寫開頭,才不會誤配物件字面值的 key: value。
      /\b(?:const|let|var)\s+\w+\s*:\s*[A-Z]\w*(?:<[^>]*>)?(?:\[\])?\s*=/u,
      // 參數或回傳值標了非原生型別。
      /\)\s*:\s*[A-Z]\w*(?:<[^>]*>)?(?:\[\])?\s*\{/u,
      /\b\w+\s*[:?]\s*[A-Z]\w*(?:<[^>]*>)?(?:\[\])?\s*[,;)]/u,
      /\bas\s+[A-Z]\w*\b/u,
      /[?]:\s*\w/u,
    ],
  },
  {
    language: "javascript",
    patterns: [
      /\b(?:const|let|var)\s+\w+\s*=/u,
      /\bfunction\s*\w*\s*\(/u,
      /[=]>\s*[{(]/u,
      /\b(?:require\s*\(|module\.exports|export\s+default)\b/u,
      /\bconsole\.(?:log|error|warn)\s*\(/u,
      /^\s*import\s+.*\s+from\s+["']/mu,
    ],
  },
  {
    language: "cpp",
    patterns: [
      /^\s*#include\s*[<"]/mu,
      /\bstd::\w+/u,
      /\b(?:using\s+namespace|template\s*<|nullptr)\b/u,
      /\bint\s+main\s*\(/u,
      /\b(?:cout|cin)\s*<</u,
    ],
  },
  {
    language: "java",
    patterns: [
      /\b(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?[\w<>[\]]+\s+\w+\s*\(/u,
      /^\s*package\s+[\w.]+;/mu,
      /^\s*import\s+(?:java|javax)\./mu,
      /\bSystem\.out\.print/u,
      /\bnew\s+[A-Z]\w*\s*\(/u,
      /\b(?:class|interface)\s+[A-Z]\w*/u,
    ],
  },
  {
    language: "sql",
    patterns: [
      /\bselect\b[\s\S]*\bfrom\b/iu,
      /\b(?:insert\s+into|update\s+\w+\s+set|delete\s+from)\b/iu,
      /\bcreate\s+(?:table|index|view|database)\b/iu,
      /\b(?:inner|left|right|outer)\s+join\b/iu,
      /\b(?:group\s+by|order\s+by|having)\b/iu,
    ],
  },
  {
    language: "css",
    patterns: [
      /^\s*[.#]?[\w-]+(?:\s*[,>+~]\s*[\w.#[\]:-]+)*\s*\{[\s\S]*\}/mu,
      /^\s*[\w-]+\s*:\s*[^;{}]+;/mu,
      /\b\d+(?:px|rem|em|vh|vw|%)\b/u,
      /^\s*@(?:media|import|keyframes|supports)\b/mu,
      /\bvar\(--[\w-]+\)/u,
    ],
  },
  {
    language: "shellscript",
    patterns: [
      /^#!.*\b(?:ba|z|k|da)?sh\b/u,
      /^\s*(?:echo|cd|export|source|sudo|chmod|mkdir|rm|grep|awk|sed|curl)\s+/mu,
      /\$\{?\w+\}?/u,
      /\|\s*(?:grep|awk|sed|xargs|head|tail|sort|jq)\b/u,
      /^\s*(?:if|for|while)\b.*;\s*(?:then|do)\b/mu,
      /\bfi\b|\bdone\b|\besac\b/u,
    ],
  },
  {
    language: "yaml",
    patterns: [
      /^---\s*$/mu,
      // 頂層一個 key 加一個縮排的 key —— 靠縮排跟「剛好有冒號的普通文字」區分。
      /^[\w.-]+:(?:\s+\S|\s*$)/mu,
      /^[ \t]+[\w.-]+:(?:\s+\S|\s*$)/mu,
      /^[ \t]*-\s+[\w"'{[]/mu,
      /^\s*#\s/mu,
    ],
  },
  {
    language: "markdown",
    patterns: [
      /^#{1,6}\s+\S/mu,
      /^\s*(?:[*-]|\d+\.)\s+\S/mu,
      /```/u,
      /\[[^\]]+\]\([^)]+\)/u,
      /\*\*\S[\s\S]*?\*\*|__\S[\s\S]*?__/u,
      /^\s*>\s+\S/mu,
    ],
  },
  {
    language: "html",
    patterns: [
      /<!doctype\s+html>/iu,
      /<\/(?:html|head|body|div|span|p|a|ul|li|table|section)>/iu,
      /<(?:div|span|p|a|img|input|br|h[1-6])\b[^>]*>/iu,
      /\b(?:class|id|href|src)\s*=\s*["']/u,
    ],
  },
];

// 這幾個語言的規則太寬鬆,中兩條可能只是巧合,所以要求更多條。
// 例如 YAML 的 key: value 跟 Markdown 的 - item 在普通文字裡也會出現。
const MIN_MATCHES: Record<string, number> = {
  css: 3,
  markdown: 2,
  yaml: 3,
};

const DEFAULT_MIN_MATCHES = 2;

// 只有某一個語言才合法的語法,中一條就直接定案,不用湊滿門檻。
// 依序檢查。
const CONCLUSIVE: readonly (readonly [string, RegExp])[] = [
  // import type / export type 在純 JS 是語法錯誤。
  ["typescript", /^[ \t]*(?:import|export)\s+type\s*[{\w*]/mu],
  ["typescript", /^[ \t]*(?:export\s+)?interface\s+\w+\s*\{/mu],
  ["typescript", /\bsatisfies\s+\w/u],
  ["php", /<\?php\b/u],
  ["rust", /\bfn\s+\w+\s*\([^)]*\)\s*->/u],
  ["rust", /\blet\s+mut\s+/u],
  ["rust", /#\[derive\(/u],
  ["go", /^[ \t]*func\s+\(\s*\w+\s+\*?\w+\s*\)\s*\w+\s*\(/mu],
  ["go", /\bif\s+err\s*!=\s*nil\b/u],
  ["cpp", /^[ \t]*#include\s*[<"]/mu],
  ["python", /^[ \t]*def\s+\w+\s*\(.*\)\s*(?:->\s*[\w[\]., |]+\s*)?:\s*$/mu],
  ["shellscript", /^#!.*\b(?:ba|z|k|da)?sh\b/u],
];

function looksLikeJson(code: string): boolean {
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
  let matches = 0;
  for (const pattern of patterns) {
    if (pattern.test(code)) {
      matches += 1;
    }
  }
  return matches;
}

/**
 * 猜貼上內容的語言。沒有把握時回傳 null,
 * 呼叫端應該維持現在的選擇,而不是退回 plaintext。
 */
export function detectLanguage(code: string): string | null {
  const trimmed = code.trim();
  if (trimmed.length < MIN_CHARS_TO_GUESS) {
    return null;
  }

  if (looksLikeJson(trimmed)) {
    return "json";
  }

  for (const [language, pattern] of CONCLUSIVE) {
    if (pattern.test(trimmed)) {
      return language;
    }
  }

  let best: { language: string; matches: number } | null = null;
  for (const rule of RULES) {
    const matches = countMatches(trimmed, rule.patterns);
    const threshold = MIN_MATCHES[rule.language] ?? DEFAULT_MIN_MATCHES;
    if (matches >= threshold && (best === null || matches > best.matches)) {
      best = { language: rule.language, matches };
    }
  }

  return best?.language ?? null;
}
