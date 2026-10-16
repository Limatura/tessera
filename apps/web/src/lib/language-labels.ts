/**
 * Language value to display name. Shared by the upload form's select and the
 * share page's default name when there is no title, so the two cannot drift.
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
  svg: "SVG",
  typescript: "TypeScript",
  yaml: "YAML",
} as const satisfies Record<string, string>;

export function languageLabel(language: string): string | null {
  return Object.hasOwn(LANGUAGE_LABELS, language)
    ? LANGUAGE_LABELS[language as keyof typeof LANGUAGE_LABELS]
    : null;
}
