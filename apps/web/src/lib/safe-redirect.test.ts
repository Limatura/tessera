import { describe, expect, it } from "vite-plus/test";

import { safeRedirect } from "./safe-redirect";

const ORIGIN = "https://tessera.test";

describe("safeRedirect", () => {
  it("no value means nowhere to go", () => {
    expect(safeRedirect(undefined, ORIGIN)).toBeNull();
    expect(safeRedirect("", ORIGIN)).toBeNull();
  });

  it("a same-origin relative path passes as is, query string and hash kept", () => {
    expect(safeRedirect("/users", ORIGIN)).toBe("/users");
    expect(safeRedirect("/users?page=2#row-3", ORIGIN)).toBe(
      "/users?page=2#row-3"
    );
  });

  it("a same-origin absolute URL keeps only its path (what admin sends)", () => {
    expect(safeRedirect(`${ORIGIN}/settings/site`, ORIGIN)).toBe(
      "/settings/site"
    );
  });

  it("another site is always refused", () => {
    expect(safeRedirect("https://evil.com/x", ORIGIN)).toBeNull();
    // Protocol-relative: it does start with a slash, but leaves the site.
    expect(safeRedirect("//evil.com", ORIGIN)).toBeNull();
  });

  it("a backslash does not pass as a same-origin relative path", () => {
    // Browsers normalise backslashes to slashes, so both equal //evil.com,
    // yet they pass a "starts with a single slash" string check.
    expect(safeRedirect("/\\evil.com", ORIGIN)).toBeNull();
    expect(safeRedirect("/\\/evil.com", ORIGIN)).toBeNull();
  });

  it("a scheme other than http is refused too", () => {
    // oxlint-disable-next-line no-script-url -- this is what must be blocked
    expect(safeRedirect("javascript:alert(1)", ORIGIN)).toBeNull();
  });
});
