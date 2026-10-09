/**
 * 語言 value → 顯示名稱。上傳表單的選單跟分享頁「沒標題時的預設名稱」共用這份,
 * 不然兩邊各寫一份遲早對不上。
 */
export const LANGUAGE_LABELS = {
  cpp: "C++",
  css: "CSS",
  go: "Go",
  html: "HTML",
  java: "Java",
  javascript: "JavaScript",
  json: "JSON",
  markdown: "Markdown",
  mermaid: "Mermaid",
  php: "PHP",
  plaintext: "Plain text",
  python: "Python",
  rust: "Rust",
  shellscript: "Shell",
  sql: "SQL",
  typescript: "TypeScript",
  yaml: "YAML",
} as const satisfies Record<string, string>;

export function languageLabel(language: string): string | null {
  return Object.hasOwn(LANGUAGE_LABELS, language)
    ? LANGUAGE_LABELS[language as keyof typeof LANGUAGE_LABELS]
    : null;
}
