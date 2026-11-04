import { describe, expect, it } from "vite-plus/test";

import { shareXConfig, shareXFileName } from "./sharex";

const ORIGIN = "https://share.example.com";

describe("shareXConfig", () => {
  const config = shareXConfig({
    name: "Tessera",
    origin: ORIGIN,
    secret: "tsk_abc",
  });

  it("uploads the raw file to the site root under its own name", () => {
    expect(config.RequestMethod).toBe("PUT");
    expect(config.Body).toBe("Binary");
    expect(config.RequestURL).toBe(`${ORIGIN}/{filename}`);
  });

  it("authorizes with the key and asks for JSON", () => {
    expect(config.Headers).toEqual({
      Accept: "application/json",
      Authorization: "Bearer tsk_abc",
    });
  });

  it("builds the link from the origin and the relative path", () => {
    expect(config.URL).toBe(`${ORIGIN}{json:path}`);
  });

  it("sets no expiry, so the instance default applies", () => {
    expect(JSON.stringify(config)).not.toContain("X-Share-Expires");
  });
});

describe("shareXFileName", () => {
  it("names the file after the host, port included", () => {
    expect(shareXFileName(ORIGIN)).toBe("tessera-share.example.com.sxcu");
    expect(shareXFileName("http://localhost:8080")).toBe(
      "tessera-localhost-8080.sxcu"
    );
  });
});
