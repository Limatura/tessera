import { describe, expect, it } from "vite-plus/test";

import {
  asBoolean,
  asEnum,
  asLines,
  asNumber,
  asScopes,
  asString,
  isSecretSet,
  toLines,
  toScopes,
} from "./settings-values";

describe("scalar readers", () => {
  it.each([
    ["x", "x"],
    ["", ""],
    [null, "fb"],
    [undefined, "fb"],
    [1, "fb"],
    [true, "fb"],
  ])("asString(%j) -> %j", (value, expected) => {
    expect(asString(value, "fb")).toBe(expected);
  });

  it.each([
    [0, 0],
    [-1, -1],
    [Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER],
    ["5", 7],
    [null, 7],
    [undefined, 7],
  ])("asNumber(%j) -> %j", (value, expected) => {
    expect(asNumber(value, 7)).toBe(expected);
  });

  it.each([
    [true, true],
    [false, false],
    ["true", true],
    [0, true],
    [null, true],
  ])("asBoolean(%j) -> %j", (value, expected) => {
    expect(asBoolean(value, true)).toBe(expected);
  });
});

describe("asEnum", () => {
  const modes = ["open", "invite", "closed"] as const;

  it("keeps a known option and falls back otherwise", () => {
    expect(asEnum("invite", modes, "open")).toBe("invite");
    expect(asEnum("INVITE", modes, "open")).toBe("open");
    expect(asEnum(null, modes, "closed")).toBe("closed");
    expect(asEnum(1, modes, "closed")).toBe("closed");
  });
});

describe("scopes", () => {
  it("joins string entries and drops the rest", () => {
    expect(asScopes(["openid", 1, "email", null], "")).toBe("openid email");
    expect(asScopes([], "fb")).toBe("");
    expect(asScopes("openid", "fb")).toBe("fb");
  });

  it("splits on any whitespace and drops empties", () => {
    expect(toScopes("  openid \t email\nprofile ")).toEqual([
      "openid",
      "email",
      "profile",
    ]);
    expect(toScopes("")).toEqual([]);
    expect(toScopes("   ")).toEqual([]);
  });

  it("round-trips", () => {
    const scopes = ["openid", "email", "profile"];
    expect(toScopes(asScopes(scopes, ""))).toEqual(scopes);
  });
});

describe("lines", () => {
  it("joins string entries one per line", () => {
    expect(asLines(["10.0.0.0/8", 3, "::1"], "")).toBe("10.0.0.0/8\n::1");
    expect(asLines(null, "fb")).toBe("fb");
  });

  it("trims and drops blank lines", () => {
    expect(toLines(" 10.0.0.0/8 \n\n  \r\n::1\r\n")).toEqual([
      "10.0.0.0/8",
      "::1",
    ]);
    expect(toLines("")).toEqual([]);
  });

  it("round-trips", () => {
    const proxies = ["10.0.0.0/8", "192.168.0.1"];
    expect(toLines(asLines(proxies, ""))).toEqual(proxies);
  });
});

describe("isSecretSet", () => {
  const settings = {
    settings: [
      { key: "mail.password", value: true },
      { key: "auth.oidc.client_secret", value: false },
      { key: "database.url", value: null },
    ],
  } as unknown as Parameters<typeof isSecretSet>[0];

  it.each([
    ["mail.password", true],
    ["auth.oidc.client_secret", false],
    ["database.url", false],
    ["missing.key", false],
  ])("%s -> %s", (key, expected) => {
    expect(isSecretSet(settings, key)).toBe(expected);
  });

  it("is false while loading", () => {
    expect(isSecretSet(undefined, "mail.password")).toBe(false);
  });
});
