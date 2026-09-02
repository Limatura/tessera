import { describe, expect, it } from "vitest";

import { safeRedirect } from "./safe-redirect";

const ORIGIN = "https://tessera.test";

describe("safeRedirect", () => {
  it("沒帶值就沒有要去的地方", () => {
    expect(safeRedirect(undefined, ORIGIN)).toBeNull();
    expect(safeRedirect("", ORIGIN)).toBeNull();
  });

  it("同源的相對路徑原樣放行,查詢字串跟 hash 都留著", () => {
    expect(safeRedirect("/users", ORIGIN)).toBe("/users");
    expect(safeRedirect("/users?page=2#row-3", ORIGIN)).toBe(
      "/users?page=2#row-3"
    );
  });

  it("同源的絕對網址只留路徑 —— admin 送過來的就是這種", () => {
    expect(safeRedirect(`${ORIGIN}/settings/site`, ORIGIN)).toBe(
      "/settings/site"
    );
  });

  it("外站一律擋掉", () => {
    expect(safeRedirect("https://evil.com/x", ORIGIN)).toBeNull();
    // protocol-relative,開頭確實是斜線但會跳去外站。
    expect(safeRedirect("//evil.com", ORIGIN)).toBeNull();
  });

  it("反斜線不能當成同源的相對路徑", () => {
    // 瀏覽器把反斜線正規化成斜線,所以這兩個都等同 //evil.com,
    // 但它們通得過「開頭是單一斜線」這種字串檢查。
    expect(safeRedirect("/\\evil.com", ORIGIN)).toBeNull();
    expect(safeRedirect("/\\/evil.com", ORIGIN)).toBeNull();
  });

  it("不是 http 的 scheme 也擋掉", () => {
    // oxlint-disable-next-line no-script-url -- 這正是要擋下來的東西
    expect(safeRedirect("javascript:alert(1)", ORIGIN)).toBeNull();
  });
});
