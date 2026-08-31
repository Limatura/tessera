import { describe, expect, it } from "vitest";

import { detectLanguage } from "./detect-language";

/**
 * 這裡的重點不是「每種語言都測到」,而是**加新規則時不會誤傷既有語言**。
 * 加語言請一併在下面補一筆,尤其是容易被搶走的(mermaid 的 `graph TD`
 * 以前會被 yaml/markdown 認走)。
 */
describe("detectLanguage", () => {
  it("內容太短時不亂猜", () => {
    expect(detectLanguage("hi")).toBeNull();
    expect(detectLanguage("")).toBeNull();
    expect(detectLanguage("   \n  ")).toBeNull();
  });

  it("認不出來的普通文字回傳 null,而不是 plaintext", () => {
    expect(
      detectLanguage("just some english prose with nothing special in it")
    ).toBeNull();
  });

  describe.each([
    ["graph TD\n  A[Start] --> B{Choice}\n  B -->|yes| C[Do]\n", "mermaid"],
    ["flowchart LR\n  a --> b\n", "mermaid"],
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
  ])("認得出語言", (source, expected) => {
    it(`把 ${JSON.stringify(source.slice(0, 24))} 認成 ${expected}`, () => {
      expect(detectLanguage(source)).toBe(expected);
    });
  });

  it("mermaid 的圖表宣告不會被 yaml 或 markdown 搶走", () => {
    // `graph TD` 同時像 yaml 的 key: value,也像 markdown 的清單。
    expect(detectLanguage("graph TD\n  A --> B\n  B --> C\n  C --> D\n")).toBe(
      "mermaid"
    );
  });
});
