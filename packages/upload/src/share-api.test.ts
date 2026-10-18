import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

import { createUploader } from "./index";

const { updateShare } = createUploader({ baseUrl: "http://tessera.test" });

/**
 * `PATCH /shares/{code}` tells "field not mentioned" from "explicitly null":
 * the first leaves it alone, the second clears it (the server uses
 * `Patch<T> = Option<Option<T>>`). The body also has `deny_unknown_fields`, so
 * one extra key fails the request.
 *
 * The password is the exception: setting it goes through the
 * `X-Share-Password` header and removing it through `remove_password` in the
 * body. The server refuses both together.
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

  it("sends only the fields the caller actually set", async () => {
    await updateShare("abc", { maxUses: 1 });
    const { body, method, url } = lastRequest();
    expect(method).toBe("PATCH");
    expect(url).toBe("http://tessera.test/api/v1/shares/abc");
    expect(body).toEqual({ max_uses: 1 });
  });

  it("an expiry of 0 or undefined is sent as null (never expires)", async () => {
    await updateShare("abc", { expiresInMinutes: 0 });
    expect(lastRequest().body).toEqual({ expires_in_minutes: null });

    await updateShare("abc", { expiresInMinutes: undefined });
    expect(lastRequest().body).toEqual({ expires_in_minutes: null });
  });

  it("setting a password uses the header, with no remove_password in the body", async () => {
    await updateShare("abc", { password: "s3cret" });
    const { body, headers } = lastRequest();
    expect(headers["X-Share-Password"]).toBe("s3cret");
    expect(body).not.toHaveProperty("remove_password");
  });

  it("removing a password uses the body and sends no password header (the server refuses both together)", async () => {
    await updateShare("abc", { password: undefined });
    const { body, headers } = lastRequest();
    expect(body).toEqual({ remove_password: true });
    expect(headers).not.toHaveProperty("X-Share-Password");
  });

  it("the body only carries keys the server knows", async () => {
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

  it("sends the management token when there is one", async () => {
    await updateShare("abc", { maxUses: 1 }, "tok-123");
    expect(lastRequest().headers["X-Share-Management-Token"]).toBe("tok-123");
  });

  it("throws the server's message on failure", async () => {
    globalThis.fetch = vi.fn<typeof fetch>(() =>
      Promise.resolve(Response.json({ message: "Nope" }, { status: 400 }))
    );
    await expect(updateShare("abc", { maxUses: 1 })).rejects.toThrow("Nope");
  });
});
