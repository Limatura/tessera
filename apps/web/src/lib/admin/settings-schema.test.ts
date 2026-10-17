import { describe, expect, it, vi } from "vite-plus/test";

import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  accountSettingsSchema,
  appearanceSettingsSchema,
  CODE_LENGTH_MAX,
  CODE_LENGTH_MIN,
  mailSettingsSchema,
  MAX_REGISTRATION_TTL_HOURS,
  MAX_RESET_TTL_MINUTES,
  MAX_TRACKED_KEYS_CEILING,
  MIN_PART_SIZE_BYTES,
  networkSettingsSchema,
  oauthSettingsSchema,
  oidcSettingsSchema,
  passwordSettingsSchema,
  scanSettingsSchema,
  shareSettingsSchema,
  SITE_DESCRIPTION_MAX,
  SITE_TITLE_MAX,
  siteSettingsSchema,
  storageSettingsSchema,
  THROTTLE_COUNTERS,
  throttleSettingsSchema,
  uploadSettingsSchema,
} from "./settings-schema";

// The `@/` alias is not set up for unit tests, and the real module pulls in
// the API error helpers; only its two bounds are needed.
const { PASSWORD_MAX_CEILING, PASSWORD_MIN_FLOOR } = vi.hoisted(() => ({
  PASSWORD_MAX_CEILING: 1024,
  PASSWORD_MIN_FLOOR: 8,
}));

vi.mock("@/lib/api/password-policy", () => ({
  PASSWORD_MAX_CEILING,
  PASSWORD_MIN_FLOOR,
}));

// Error messages are generated per build; any settings_schema_* returns its key.
const { isMessage } = vi.hoisted(() => ({
  isMessage: (key: string | symbol): key is string =>
    typeof key === "string" && key.startsWith("settings_schema_"),
}));

vi.mock(
  "@/paraglide/messages",
  () =>
    new Proxy(
      {},
      {
        get: (_, key) => (isMessage(key) ? () => key : undefined),
        has: (_, key) => isMessage(key),
      }
    )
);

/** The paths of every issue, or `[]` when the value parses. */
function issuePaths(
  schema: {
    safeParse: (value: unknown) => {
      error?: { issues: { path: PropertyKey[] }[] };
    };
  },
  value: unknown
): string[] {
  return (
    schema
      .safeParse(value)
      .error?.issues.map((issue) => issue.path.join(".")) ?? []
  );
}

describe("appearanceSettingsSchema", () => {
  const valid = {
    accent: APPEARANCE_ACCENTS[0],
    mode: APPEARANCE_MODES[0],
    theme: APPEARANCE_THEMES[0],
  };

  it("accepts every listed option", () => {
    expect(issuePaths(appearanceSettingsSchema, valid)).toEqual([]);
  });

  it("rejects an unknown option", () => {
    expect(
      issuePaths(appearanceSettingsSchema, { ...valid, mode: "sepia" })
    ).toEqual(["mode"]);
  });
});

describe("siteSettingsSchema", () => {
  const valid = { description: "", language: "en", title: "Tessera" };

  it.each([
    [{}, []],
    [{ title: "x".repeat(SITE_TITLE_MAX) }, []],
    [{ title: "x".repeat(SITE_TITLE_MAX + 1) }, ["title"]],
    // Emoji count as one code point each, as the server counts them
    [{ title: "😀".repeat(SITE_TITLE_MAX) }, []],
    [{ title: "" }, ["title"]],
    [{ title: "   " }, ["title"]],
    [{ description: "y".repeat(SITE_DESCRIPTION_MAX) }, []],
    [{ description: "y".repeat(SITE_DESCRIPTION_MAX + 1) }, ["description"]],
    [{ language: "fr" }, ["language"]],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(siteSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});

describe("storageSettingsSchema", () => {
  const valid = {
    partSizeBytes: MIN_PART_SIZE_BYTES,
    reconcileGraceHours: 0,
    reconcileIntervalHours: 0,
    splitAcrossBackends: false,
  };

  it.each([
    [{}, []],
    [{ partSizeBytes: MIN_PART_SIZE_BYTES - 1 }, ["partSizeBytes"]],
    [{ partSizeBytes: MIN_PART_SIZE_BYTES + 0.5 }, ["partSizeBytes"]],
    [{ reconcileGraceHours: -1 }, ["reconcileGraceHours"]],
    [{ reconcileIntervalHours: -1 }, ["reconcileIntervalHours"]],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(storageSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});

describe("uploadSettingsSchema", () => {
  const valid = {
    allowAnonymousUpload: true,
    defaultExpiryHours: 24,
    maxExpiryHours: 48,
    maxTotalBytes: 0,
  };

  it.each([
    [{}, []],
    [{ defaultExpiryHours: 48 }, []],
    [{ defaultExpiryHours: 49 }, ["defaultExpiryHours"]],
    // 0 on either side means "no limit" / "use the max"
    [{ defaultExpiryHours: 1000, maxExpiryHours: 0 }, []],
    [{ defaultExpiryHours: 0, maxExpiryHours: 1 }, []],
    [{ maxTotalBytes: -1 }, ["maxTotalBytes"]],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(uploadSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});

describe("accountSettingsSchema", () => {
  const valid = {
    defaultRole: "member",
    loginHistoryDays: 1,
    registration: "invite",
    sessionTtlHours: 1,
  };

  it.each([
    [{}, []],
    [{ defaultRole: "" }, ["defaultRole"]],
    [{ loginHistoryDays: 0 }, ["loginHistoryDays"]],
    // MAX_LOGIN_HISTORY_DAYS in apps/server/src/config.rs
    [{ loginHistoryDays: 730 }, []],
    [{ loginHistoryDays: 731 }, ["loginHistoryDays"]],
    [{ sessionTtlHours: 0 }, ["sessionTtlHours"]],
    [{ registration: "public" }, ["registration"]],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(accountSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});

describe("oidcSettingsSchema", () => {
  const valid = {
    allowRegistration: "inherit",
    clientId: "",
    clientSecret: "",
    displayName: "",
    groupClaim: "groups",
    issuer: "",
    linkByEmail: "never",
    requiredGroup: "",
    scopes: "openid",
  };

  it.each([
    ["", []],
    ["https://id.example.com", []],
    ["https://id.example.com/realms/x", []],
    ["http://id.example.com", ["issuer"]],
    // is_private_or_tls in apps/server/src/auth/oidc/mod.rs: http on loopback only
    ["http://127.0.0.1:8080", []],
    ["http://localhost:9000/realms/main", []],
    ["http://[::1]", []],
    ["http://127.0.0.1.evil.example", ["issuer"]],
    ["http://localhost.evil.example", ["issuer"]],
    ["https:///nohost", ["issuer"]],
    ["https://id.example.com/.well-known/openid-configuration", ["issuer"]],
  ])("issuer %j -> %j", (issuer, expected) => {
    expect(issuePaths(oidcSettingsSchema, { ...valid, issuer })).toEqual(
      expected
    );
  });

  // OidcConfig::validate in apps/server/src/config.rs
  it("requires a group claim once an issuer is set", () => {
    const issuer = "https://id.example.com";
    expect(
      issuePaths(oidcSettingsSchema, { ...valid, groupClaim: " ", issuer })
    ).toEqual(["groupClaim"]);
    expect(
      issuePaths(oidcSettingsSchema, { ...valid, groupClaim: "", issuer: "" })
    ).toEqual([]);
  });

  it("rejects an unknown link mode", () => {
    expect(
      issuePaths(oidcSettingsSchema, { ...valid, linkByEmail: "sometimes" })
    ).toEqual(["linkByEmail"]);
  });
});

describe("oauthSettingsSchema", () => {
  const valid = {
    allowRegistration: "never",
    baseUrl: "",
    clientId: "",
    clientSecret: "",
    displayName: "",
    linkByVerifiedEmail: false,
    scopes: "",
  };

  it.each([
    ["", []],
    ["http://ghe.local", []],
    ["https://ghe.example.com/api", []],
    ["ghe.example.com", ["baseUrl"]],
    ["ftp://ghe.example.com", ["baseUrl"]],
  ])("baseUrl %j -> %j", (baseUrl, expected) => {
    expect(issuePaths(oauthSettingsSchema, { ...valid, baseUrl })).toEqual(
      expected
    );
  });
});

describe("scanSettingsSchema", () => {
  it.each([
    ["", []],
    ["clamd://127.0.0.1:3310", []],
    ["https://scan.example.com/v1", []],
    ["  clamd://host:3310  ", []],
    ["tcp://host:3310", ["url"]],
    ["clamd://", ["url"]],
    // ScanConfig::validate in apps/server/src/config.rs: clamd needs host and port
    ["clamd://127.0.0.1", ["url"]],
    ["clamd://[::1]:3310", []],
    ["clamd://:3310", ["url"]],
  ])("url %j -> %j", (url, expected) => {
    expect(issuePaths(scanSettingsSchema, { authorization: "", url })).toEqual(
      expected
    );
  });

  it("trims the url", () => {
    expect(
      scanSettingsSchema.parse({ authorization: "", url: " clamd://h:1 " }).url
    ).toBe("clamd://h:1");
  });
});

describe("mailSettingsSchema", () => {
  const off = {
    from: "",
    host: "",
    password: "",
    port: 587,
    publicUrl: "",
    registrationTtlHours: 1,
    resetTtlMinutes: 1,
    tls: "auto",
    username: "",
  };
  const on = {
    ...off,
    from: "noreply@example.com",
    host: "smtp.example.com",
    publicUrl: "https://example.com",
  };

  it.each([
    [off, []],
    [on, []],
    [{ ...on, port: 0 }, ["port"]],
    [{ ...on, port: 65_535 }, []],
    [{ ...on, port: 65_536 }, ["port"]],
    [{ ...on, resetTtlMinutes: MAX_RESET_TTL_MINUTES }, []],
    [
      { ...on, resetTtlMinutes: MAX_RESET_TTL_MINUTES + 1 },
      ["resetTtlMinutes"],
    ],
    [{ ...on, resetTtlMinutes: 0 }, ["resetTtlMinutes"]],
    [{ ...on, registrationTtlHours: MAX_REGISTRATION_TTL_HOURS }, []],
    [
      { ...on, registrationTtlHours: MAX_REGISTRATION_TTL_HOURS + 1 },
      ["registrationTtlHours"],
    ],
    [{ ...on, from: "noreply" }, ["from"]],
    [{ ...on, publicUrl: "example.com" }, ["publicUrl"]],
    [{ ...on, tls: "ssl" }, ["tls"]],
  ])("%j -> %j", (value, expected) => {
    expect(issuePaths(mailSettingsSchema, value)).toEqual(expected);
  });

  it("names every missing field of a partial host/from/publicUrl setup", () => {
    expect(
      issuePaths(mailSettingsSchema, { ...off, host: "smtp.example.com" })
    ).toEqual(["from", "publicUrl"]);
    expect(issuePaths(mailSettingsSchema, { ...on, publicUrl: "   " })).toEqual(
      ["publicUrl"]
    );
  });
});

describe("networkSettingsSchema", () => {
  const valid = {
    cookieSecure: "auto",
    publicUrl: "",
    requestTimeoutSecs: 1,
    trustedProxies: "",
    uploadStallSecs: 1,
  };

  it.each([
    [{}, []],
    [{ publicUrl: "https://files.example.com" }, []],
    [{ publicUrl: "files.example.com" }, ["publicUrl"]],
    [{ requestTimeoutSecs: 0 }, ["requestTimeoutSecs"]],
    [{ uploadStallSecs: 0 }, ["uploadStallSecs"]],
    [{ cookieSecure: "sometimes" }, ["cookieSecure"]],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(networkSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});

describe("throttleSettingsSchema", () => {
  const valid: Record<string, number> = { maxTrackedKeys: 1 };
  for (const counter of THROTTLE_COUNTERS) {
    valid[`${counter}MaxAttempts`] = 1;
    valid[`${counter}WindowSecs`] = 1;
  }

  it("covers every counter and accepts the floor", () => {
    expect(Object.keys(throttleSettingsSchema.shape).toSorted()).toEqual(
      Object.keys(valid).toSorted()
    );
    expect(issuePaths(throttleSettingsSchema, valid)).toEqual([]);
  });

  it.each(THROTTLE_COUNTERS)("rejects a zeroed %s pair", (counter) => {
    expect(
      issuePaths(throttleSettingsSchema, {
        ...valid,
        [`${counter}MaxAttempts`]: 0,
        [`${counter}WindowSecs`]: 0,
      })
    ).toEqual([`${counter}MaxAttempts`, `${counter}WindowSecs`]);
  });

  it.each([
    [0, ["maxTrackedKeys"]],
    [MAX_TRACKED_KEYS_CEILING, []],
    [MAX_TRACKED_KEYS_CEILING + 1, ["maxTrackedKeys"]],
  ])("maxTrackedKeys %d -> %j", (maxTrackedKeys, expected) => {
    expect(
      issuePaths(throttleSettingsSchema, { ...valid, maxTrackedKeys })
    ).toEqual(expected);
  });
});

describe("passwordSettingsSchema", () => {
  const valid = {
    iterations: 1,
    lanes: 1,
    maxLength: PASSWORD_MAX_CEILING,
    // AuthHashing::validate in apps/server/src/config.rs: at least 7168
    memoryKib: 7168,
    minLength: PASSWORD_MIN_FLOOR,
    pwnedEnabled: false,
    pwnedUrl: "",
    requireDigit: false,
    requireMixedCase: false,
    requireSymbol: false,
  };

  it.each([
    [{}, []],
    [{ minLength: PASSWORD_MIN_FLOOR - 1 }, ["minLength"]],
    [{ maxLength: PASSWORD_MAX_CEILING + 1 }, ["maxLength"]],
    [{ maxLength: PASSWORD_MIN_FLOOR, minLength: PASSWORD_MIN_FLOOR }, []],
    [
      { maxLength: PASSWORD_MIN_FLOOR, minLength: PASSWORD_MIN_FLOOR + 1 },
      ["minLength"],
    ],
    [{ memoryKib: 7167 }, ["memoryKib"]],
    [{ iterations: 0 }, ["iterations"]],
    [{ lanes: 0 }, ["lanes"]],
    [{ pwnedEnabled: true }, ["pwnedUrl"]],
    [{ pwnedEnabled: true, pwnedUrl: "https://api.pwnedpasswords.com" }, []],
    [{ pwnedUrl: "not a url" }, []],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(passwordSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});

describe("shareSettingsSchema", () => {
  const valid = {
    accessLogEnabled: false,
    accessLogRetentionDays: 0,
    codeLength: CODE_LENGTH_MIN,
    encryption: false,
    inlineThresholdBytes: 0,
    maxTextBytes: 1,
    retainAfterDeathHours: 0,
    ticketIdleHours: 1,
    ticketTtlHours: 1,
    uploadSessionTtlHours: 1,
  };

  it.each([
    [{}, []],
    [{ codeLength: CODE_LENGTH_MIN - 1 }, ["codeLength"]],
    [{ codeLength: CODE_LENGTH_MAX }, []],
    [{ codeLength: CODE_LENGTH_MAX + 1 }, ["codeLength"]],
    [{ accessLogEnabled: true }, ["accessLogRetentionDays"]],
    [{ accessLogEnabled: true, accessLogRetentionDays: 1 }, []],
    [{ maxTextBytes: 0 }, ["maxTextBytes"]],
    [{ ticketIdleHours: 0 }, ["ticketIdleHours"]],
    [{ inlineThresholdBytes: -1 }, ["inlineThresholdBytes"]],
    // MAX_TEXT_CEILING / INLINE_CEILING in apps/server/src/config.rs
    [{ maxTextBytes: 16 * 1024 * 1024 }, []],
    [{ maxTextBytes: 16 * 1024 * 1024 + 1 }, ["maxTextBytes"]],
    [{ inlineThresholdBytes: 1024 * 1024 }, []],
    [{ inlineThresholdBytes: 1024 * 1024 + 1 }, ["inlineThresholdBytes"]],
  ])("%j -> %j", (patch, expected) => {
    expect(issuePaths(shareSettingsSchema, { ...valid, ...patch })).toEqual(
      expected
    );
  });
});
