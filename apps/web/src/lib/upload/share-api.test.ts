import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

import { updateShare } from "./share-api";

/**
 * `PATCH /shares/{code}` 分辨「沒提到這個欄位」和「明確送 null」:前者不動、
 * 後者清除(後端是 `Patch<T> = Option<Option<T>>`)。而且 body 有
 * `deny_unknown_fields`,多送一個 key 整個請求會被拒。
 *
 * 密碼是特例:設定走 `X-Share-Password` header,移除走 body 的
 * `remove_password`,後端明講兩個一起送會被拒絕。
 */
function lastRequest() {
  const mock = vi.mocked(globalThis.fetch);
  const [url, init] = mock.mock.calls.at(-1) as [string, RequestInit];
  return {
    body: JSON.parse(init.body as string) as Record<string, unknown>,
    headers: init.headers as Record<string, string>,
    method: init.method,
    url,
  };
}

describe("updateShare", () => {
  beforeEach(() => {
    globalThis.fetch = vi.fn<typeof fetch>(() =>
      Promise.resolve(new Response(null, { status: 200 }))
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("只送出呼叫端真的指定的欄位", async () => {
    await updateShare("abc", { maxUses: 1 });
    const { body, method, url } = lastRequest();
    expect(method).toBe("PATCH");
    expect(url).toBe("/api/v1/shares/abc");
    expect(body).toEqual({ max_uses: 1 });
  });

  it("過期時間 0 或 undefined 都送 null(代表不再過期)", async () => {
    await updateShare("abc", { expiresInMinutes: 0 });
    expect(lastRequest().body).toEqual({ expires_in_minutes: null });

    await updateShare("abc", { expiresInMinutes: undefined });
    expect(lastRequest().body).toEqual({ expires_in_minutes: null });
  });

  it("設定密碼走 header,body 不帶 remove_password", async () => {
    await updateShare("abc", { password: "s3cret" });
    const { body, headers } = lastRequest();
    expect(headers["X-Share-Password"]).toBe("s3cret");
    expect(body).not.toHaveProperty("remove_password");
  });

  it("移除密碼走 body,而且不送密碼 header(後端會拒絕兩個一起送)", async () => {
    await updateShare("abc", { password: undefined });
    const { body, headers } = lastRequest();
    expect(body).toEqual({ remove_password: true });
    expect(headers).not.toHaveProperty("X-Share-Password");
  });

  it("body 只會出現後端認得的 key", async () => {
    await updateShare("abc", {
      expiresInMinutes: 30,
      maxUses: 2,
      password: undefined,
    });
    expect(Object.keys(lastRequest().body).toSorted()).toEqual([
      "expires_in_minutes",
      "max_uses",
      "remove_password",
    ]);
  });

  it("有 management token 就帶上", async () => {
    await updateShare("abc", { maxUses: 1 }, "tok-123");
    expect(lastRequest().headers["X-Share-Management-Token"]).toBe("tok-123");
  });

  it("失敗時丟出後端給的訊息", async () => {
    globalThis.fetch = vi.fn<typeof fetch>(() =>
      Promise.resolve(Response.json({ message: "Nope" }, { status: 400 }))
    );
    await expect(updateShare("abc", { maxUses: 1 })).rejects.toThrow("Nope");
  });
});
