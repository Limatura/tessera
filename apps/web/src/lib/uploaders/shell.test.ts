import { describe, expect, it } from "vite-plus/test";

import { shellScript } from "./shell";

describe("shellScript", () => {
  const script = shellScript({
    origin: "https://share.example.com",
    secret: "tsk_abc",
  });

  it("writes the instance and the key in as defaults", () => {
    expect(script).toContain("TESSERA_URL:-'https://share.example.com'}");
    expect(script).toContain("TESSERA_TOKEN:-'tsk_abc'}");
    expect(script).not.toContain("__TESSERA_");
  });

  it("is still a POSIX sh script", () => {
    expect(script.startsWith("#!/bin/sh\n")).toBe(true);
  });

  it("sets no expiry, so the instance default applies", () => {
    expect(script).not.toContain("X-Share-Expires");
  });

  it("keeps a quote in a value from ending the string", () => {
    const quoted = shellScript({ origin: "https://a'b", secret: "x" });
    expect(quoted).toContain("TESSERA_URL:-'https://a'\\''b'}");
  });
});
