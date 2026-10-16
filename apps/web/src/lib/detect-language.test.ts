import { describe, expect, it } from "vite-plus/test";

import { detectLanguage } from "./detect-language";

/**
 * The point is not to cover every language but to catch a new rule breaking an
 * existing one. Add a case below with each new language, especially ones that
 * are easily stolen (mermaid's `graph TD` used to be taken by yaml or markdown).
 */
describe("detectLanguage", () => {
  it("does not guess when the content is too short", () => {
    expect(detectLanguage("hi")).toBeNull();
    expect(detectLanguage("")).toBeNull();
    expect(detectLanguage("   \n  ")).toBeNull();
  });

  it("returns null for ordinary text it cannot place, not plaintext", () => {
    expect(
      detectLanguage("just some english prose with nothing special in it")
    ).toBeNull();
  });

  describe.each([
    ["graph TD\n  A[Start] --> B{Choice}\n  B -->|yes| C[Do]\n", "mermaid"],
    ["flowchart LR\n  a --> b\n", "mermaid"],
    [
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle r="4"/></svg>',
      "svg",
    ],
    [
      '<?xml version="1.0"?>\n<!-- icon -->\n<svg width="10" height="10"></svg>',
      "svg",
    ],
    ["sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi\n", "mermaid"],
    ["classDiagram\n  Animal <|-- Duck\n", "mermaid"],
    ["stateDiagram-v2\n  [*] --> Still\n  Still --> Moving\n", "mermaid"],
    ["erDiagram\n  A ||--o{ B : has\n", "mermaid"],
    ["gantt\n  title A\n  section S\n  t1: 2024-01-01, 3d\n", "mermaid"],
    ["mindmap\n  root((hi))\n    a\n    b\n", "mermaid"],
    ['{"a": 1, "b": [2, 3]}', "json"],
    ["def f(x):\n    return x + 1\n\nprint(f(2))\n", "python"],
    [
      "const a = 1;\nfunction f() { console.log(a); }\nexport default f;\n",
      "javascript",
    ],
    ["interface A { b: string }\nexport type C = A;\n", "typescript"],
    ['fn main() {\n    let mut x = 1;\n    println!("{}", x);\n}\n', "rust"],
    [
      'package main\n\nimport (\n\t"fmt"\n)\n\nfunc main() { fmt.Println("x") }\n',
      "go",
    ],
    ["#include <stdio.h>\nint main() { return 0; }\n", "cpp"],
    ["SELECT id, name FROM users WHERE id = 1 ORDER BY name;\n", "sql"],
    [
      '<!doctype html>\n<html><body><div class="a">hi</div></body></html>\n',
      "html",
    ],
    ["#!/bin/bash\necho hi\ncd /tmp\n", "shellscript"],
    ["<?php\n$a = 1;\necho $a;\n", "php"],
    ["# Title\n\nSome *text* here.\n\n- a\n- b\n", "markdown"],
  ])("recognises the language", (source, expected) => {
    it(`reads ${JSON.stringify(source.slice(0, 24))} as ${expected}`, () => {
      expect(detectLanguage(source)).toBe(expected);
    });
  });

  it("a mermaid diagram declaration is not taken for yaml or markdown", () => {
    // `graph TD` looks like both a yaml `key: value` and a markdown list.
    expect(detectLanguage("graph TD\n  A --> B\n  B --> C\n  C --> D\n")).toBe(
      "mermaid"
    );
  });
});
