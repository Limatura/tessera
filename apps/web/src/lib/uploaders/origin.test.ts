import { describe, expect, it } from "vite-plus/test";

import { uploaderOrigin } from "./origin";

const PAGE = "http://localhost:8080";

describe("uploaderOrigin", () => {
  it("prefers the configured public URL over the page's origin", () => {
    expect(uploaderOrigin("https://share.example.com", PAGE)).toBe(
      "https://share.example.com"
    );
  });

  it("drops a trailing slash so paths can be appended as is", () => {
    expect(uploaderOrigin("https://share.example.com/", PAGE)).toBe(
      "https://share.example.com"
    );
  });

  it("falls back to the page's origin when none is configured", () => {
    expect(uploaderOrigin(null, PAGE)).toBe(PAGE);
    expect(uploaderOrigin(undefined, PAGE)).toBe(PAGE);
    expect(uploaderOrigin("  ", PAGE)).toBe(PAGE);
  });
});
