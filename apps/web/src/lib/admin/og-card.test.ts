import { describe, expect, it } from "vite-plus/test";

import {
  CARD_STATES,
  PLACEHOLDERS,
  STARTER_LAYOUT,
  fill,
  layoutBytes,
  sampleValues,
} from "./og-card";

const EVERYTHING = PLACEHOLDERS.map((name) => `[{{${name}}}]`).join("");

describe("fill", () => {
  it("escapes values, so a title cannot restructure the layout", () => {
    expect(fill(`<div>{{title}}</div>`, { title: `"><span>x` })).toBe(
      "<div>&quot;&gt;&lt;span&gt;x</div>"
    );
  });

  it("drops a placeholder with no value and tolerates spaces inside braces", () => {
    expect(fill("a{{ size }}b{{title}}c", { title: "T" })).toBe("abTc");
  });

  it("keeps an unclosed brace pair as written", () => {
    expect(fill("a{{title", { title: "T" })).toBe("a{{title");
  });
});

describe("sampleValues", () => {
  it("gives a locked share nothing but its title, label, footer and code", () => {
    expect(fill(EVERYTHING, sampleValues("locked", "en", "Q3 salaries"))).toBe(
      "[Q3 salaries][PASSWORD PROTECTED][][K3P9XQ2M][][Enter the password to see what this is][]"
    );
  });

  it("falls back to a stand-in title when the share has none", () => {
    expect(sampleValues("locked", "zh-TW", "  ").title).toBe(
      "受密碼保護的分享"
    );
  });

  it("never shows the uploader's title on a sealed or gone card", () => {
    for (const state of ["sealed", "gone"] as const) {
      expect(sampleValues(state, "en", "secret.pdf").title).not.toBe(
        "secret.pdf"
      );
    }
  });

  it("fills every state in every language", () => {
    for (const state of CARD_STATES) {
      for (const language of ["en", "zh-TW", "ja"] as const) {
        const values = sampleValues(state, language, "");
        expect(values.code).toBeTruthy();
        expect(values.label).toBeTruthy();
        expect(values.footer).toBeTruthy();
      }
    }
  });
});

describe("layoutBytes", () => {
  it("counts UTF-8 bytes, as the server does", () => {
    expect(layoutBytes("中")).toBe(3);
  });

  it("leaves the starter layout well inside the limit", () => {
    expect(layoutBytes(STARTER_LAYOUT)).toBeLessThan(2048);
  });
});
