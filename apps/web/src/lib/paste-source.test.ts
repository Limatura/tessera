import { describe, expect, it } from "vite-plus/test";

import { readPasteSource } from "./paste-source";

function clipboard(entries: Record<string, string>, files: File[] = []) {
  return {
    files,
    getData: (type: string) => entries[type] ?? "",
    types: Object.keys(entries),
  } as unknown as DataTransfer;
}

describe("readPasteSource", () => {
  it("reads the language VS Code writes", () => {
    const data = clipboard({
      "text/plain": "const a = 1;",
      "vscode-editor-data": JSON.stringify({
        mode: "typescriptreact",
        version: 1,
      }),
    });
    expect(readPasteSource(data)).toEqual({
      kind: "vscode",
      language: "typescript",
    });
  });

  it("ignores malformed VS Code metadata", () => {
    const data = clipboard({ "vscode-editor-data": "{not json" });
    expect(readPasteSource(data)).toEqual({ kind: "vscode", language: null });
  });

  it("takes the filename and extension of a pasted file", () => {
    const data = clipboard({}, [new File(["x"], "flow.mmd")]);
    expect(readPasteSource(data)).toEqual({
      filename: "flow.mmd",
      kind: "file",
      language: "mermaid",
    });
  });

  it("reads the language class from a web code block", () => {
    const data = clipboard({
      "text/html":
        '<pre><code class="hljs language-python">print(1)</code></pre>',
    });
    expect(readPasteSource(data)).toEqual({ kind: "html", language: "python" });
  });

  it("returns null for languages the menu does not have", () => {
    const data = clipboard({
      "vscode-editor-data": JSON.stringify({ mode: "haskell" }),
    });
    expect(readPasteSource(data).language).toBeNull();
  });

  it("falls back to plain text", () => {
    expect(readPasteSource(clipboard({ "text/plain": "hi" }))).toEqual({
      kind: "plain",
      language: null,
    });
  });
});
