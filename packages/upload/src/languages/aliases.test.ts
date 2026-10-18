import { describe, expect, it } from "vite-plus/test";

import {
  languageFromClassName,
  languageFromPath,
  normalizeLanguage,
} from "./aliases";

describe("normalizeLanguage", () => {
  it("maps other tools' names and extensions to ours, whatever their case", () => {
    expect(normalizeLanguage("ts")).toBe("typescript");
    expect(normalizeLanguage("TSX")).toBe("typescript");
    expect(normalizeLanguage("yml")).toBe("yaml");
    expect(normalizeLanguage("bash")).toBe("shellscript");
  });

  it("returns null for a name it does not know, so detection still gets a turn", () => {
    expect(normalizeLanguage("cobol")).toBeNull();
    // Not a key of the table, even though every object has it.
    expect(normalizeLanguage("constructor")).toBeNull();
  });
});

describe("languageFromClassName", () => {
  it.each([
    ["language-rust", "rust"],
    ["hljs language-js", "javascript"],
    ["lang-py s-code-block", "python"],
    ["highlight highlight-source-shell notranslate", "shellscript"],
    ["language-c++", "cpp"],
  ])("reads %s", (className, expected) => {
    expect(languageFromClassName(className)).toBe(expected);
  });

  it("skips a class it does not know for a later one it does", () => {
    expect(languageFromClassName("language-unknown lang-go")).toBe("go");
  });

  it("returns null when nothing names a language", () => {
    expect(languageFromClassName("")).toBeNull();
    expect(languageFromClassName("code-block highlighted")).toBeNull();
    expect(languageFromClassName("language-cobol")).toBeNull();
  });
});

describe("languageFromPath", () => {
  it.each([
    ["link.rs", "rust"],
    ["/Limatura/tessera/blob/main/apps/web/src/main.tsx", "typescript"],
    ["docker-compose.yml", "yaml"],
    ["archive.tar.sh", "shellscript"],
  ])("reads %s", (path, expected) => {
    expect(languageFromPath(path)).toBe(expected);
  });

  it("returns null without an extension it knows", () => {
    expect(languageFromPath("Dockerfile")).toBeNull();
    expect(languageFromPath("/blob/main/notes.txt")).toBeNull();
    // A dot in a directory is not an extension.
    expect(languageFromPath("/v1.2/README")).toBeNull();
  });
});
