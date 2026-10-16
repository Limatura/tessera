import { describe, expect, it } from "vite-plus/test";

import { settingsSectionOf } from "./settings-sections";

describe("settingsSectionOf", () => {
  it("puts a specific key before its prefix", () => {
    expect(settingsSectionOf("share.default_expiry_hours")).toBe(
      "/admin/settings/upload"
    );
    expect(settingsSectionOf("share.code_length")).toBe(
      "/admin/settings/share"
    );
    expect(settingsSectionOf("storage.max_total_bytes")).toBe(
      "/admin/settings/upload"
    );
    expect(settingsSectionOf("storage.split_across_backends")).toBe(
      "/admin/settings/storage"
    );
  });

  it("follows keys split across pages", () => {
    expect(settingsSectionOf("auth.throttle.login.window_secs")).toBe(
      "/admin/settings/throttle"
    );
    expect(settingsSectionOf("auth.cookie_secure")).toBe(
      "/admin/settings/network"
    );
    expect(settingsSectionOf("auth.oidc.issuer")).toBe(
      "/admin/settings/auth-providers"
    );
  });

  it("has no page for keys no form edits", () => {
    expect(settingsSectionOf("speed.enabled")).toBeNull();
    expect(settingsSectionOf("auth.admin_email")).toBeNull();
    expect(settingsSectionOf("server.trusted_proxy")).toBeNull();
  });
});
