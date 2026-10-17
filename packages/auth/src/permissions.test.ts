import { describe, expect, it } from "vite-plus/test";

import type { Account, Permission } from "./permissions";
import {
  canAccessAdmin,
  hasAnyPermission,
  hasPermission,
  isUnassigned,
  PERMISSIONS,
} from "./permissions";

function account(permissions: string[], roles: string[] = []): Account {
  return { permissions, roles } as unknown as Account;
}

const ALL = Object.values(PERMISSIONS);

describe("hasPermission", () => {
  it.each(ALL)("grants %s only to an account holding it", (permission) => {
    expect(hasPermission(account([permission]), permission)).toBe(true);
    expect(
      hasPermission(account(ALL.filter((p) => p !== permission)), permission)
    ).toBe(false);
  });

  it.each([null, undefined])("denies a missing account (%s)", (missing) => {
    for (const permission of ALL) {
      expect(hasPermission(missing, permission)).toBe(false);
    }
  });

  it("denies unknown or near-miss permission strings", () => {
    const holder = account(["shares:read", "SHARES:READ:ANY", "*"]);
    expect(hasPermission(holder, PERMISSIONS.sharesReadAny)).toBe(false);
    expect(hasPermission(holder, "shares:*" as Permission)).toBe(false);
    expect(hasPermission(holder, "" as Permission)).toBe(false);
  });

  it("does not treat one permission as implying another", () => {
    expect(
      hasPermission(account(["shares:read:any"]), PERMISSIONS.sharesRead)
    ).toBe(false);
    expect(
      hasPermission(account(["settings:write"]), PERMISSIONS.settingsRead)
    ).toBe(false);
  });
});

describe("hasAnyPermission", () => {
  it("is false for an empty list", () => {
    expect(hasAnyPermission(account(ALL), [])).toBe(false);
  });

  it("is true when one of several matches", () => {
    expect(
      hasAnyPermission(account(["roles:read"]), [
        PERMISSIONS.usersRead,
        PERMISSIONS.rolesRead,
      ])
    ).toBe(true);
  });

  it("is false for a missing account", () => {
    expect(hasAnyPermission(null, ALL)).toBe(false);
  });
});

describe("canAccessAdmin", () => {
  it.each([
    ["stats:read", true],
    ["users:read", true],
    ["invites:read", true],
    ["roles:read", true],
    ["shares:read:any", true],
    ["settings:read", true],
    ["shares:create", false],
    ["shares:read", false],
    ["tokens:manage", false],
    ["users:write", false],
    ["settings:write", false],
    ["unknown:thing", false],
  ])("%s -> %s", (permission, expected) => {
    expect(canAccessAdmin(account([permission]))).toBe(expected);
  });

  it("denies no account and an empty permission set", () => {
    expect(canAccessAdmin(null)).toBe(false);
    expect(canAccessAdmin(account([]))).toBe(false);
  });
});

describe("isUnassigned", () => {
  it.each([
    [null, true],
    [undefined, true],
    [account([], []), true],
    [account([], ["guest"]), false],
    [account(ALL, ["owner"]), false],
  ])("%j -> %s", (who, expected) => {
    expect(isUnassigned(who)).toBe(expected);
  });
});
