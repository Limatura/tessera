import { describe, expect, it, vi } from "vite-plus/test";

import type { Account, Permission } from "./permissions";
import {
  accountRoleLabel,
  canAccessAdmin,
  defaultRoleDescription,
  hasAnyPermission,
  hasDefaultRoleDescription,
  hasPermission,
  HIGH_RISK_PERMISSIONS,
  isUnassigned,
  landingRoute,
  permissionGroupLabel,
  permissionHint,
  permissionLabel,
  PERMISSIONS,
  roleDescription,
  roleLabel,
} from "./permissions";

// Messages are generated per build; each fake returns its own key.
const { isMessage } = vi.hoisted(() => ({
  isMessage: (key: string | symbol): key is string =>
    typeof key === "string" &&
    (key.startsWith("role_") || key.startsWith("permission_")),
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

describe("landingRoute", () => {
  it.each([
    [["stats:read"], "/admin/dashboard"],
    [["users:read"], "/admin/users"],
    [["invites:read"], "/admin/invites"],
    [["roles:read"], "/admin/roles"],
    [["shares:read:any"], "/admin/all-shares"],
    [["settings:read"], "/admin/settings"],
    [["settings:read", "users:read"], "/admin/users"],
    [ALL, "/admin/dashboard"],
  ])("%j -> %s", (permissions, expected) => {
    expect(landingRoute(account(permissions))).toBe(expected);
  });

  it.each([
    [null],
    [undefined],
    [account([])],
    [account(["shares:create", "shares:read", "tokens:manage"])],
  ])("has nowhere to land for %j", (who) => {
    expect(landingRoute(who)).toBeNull();
  });

  it("lands somewhere for exactly the admin entry permissions", () => {
    for (const permission of ALL) {
      const who = account([permission]);
      expect(landingRoute(who) !== null).toBe(canAccessAdmin(who));
    }
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

describe("role labels", () => {
  it.each([
    ["owner", "role_owner"],
    ["admin", "role_admin"],
    ["member", "role_member"],
    ["guest", "role_guest"],
    ["custom-role", "custom-role"],
  ])("roleLabel(%s) -> %s", (role, expected) => {
    expect(roleLabel(role)).toBe(expected);
  });

  it("roleLabel keeps a role named like a prototype member", () => {
    expect(roleLabel("toString")).toBe("toString");
  });

  it("permissionLabel falls back for prototype member names", () => {
    expect(permissionLabel("constructor")).toBe("constructor");
    expect(permissionHint("constructor")).toBeUndefined();
    expect(permissionGroupLabel("toString")).toBe("toString");
  });

  it("tells no role apart from guest", () => {
    expect(accountRoleLabel(account([], []))).toBe("role_unassigned");
    expect(accountRoleLabel(null)).toBe("role_unassigned");
    expect(accountRoleLabel(account([], ["guest"]))).toBe("role_guest");
    expect(accountRoleLabel(account([], ["ops"]))).toBe("ops");
  });
});

describe("role descriptions", () => {
  const seeded = "Signed in, and allowed nothing.";

  it.each([
    [{ description: seeded, is_system: true, name: "guest" }, true],
    [{ description: "", is_system: true, name: "guest" }, true],
    [{ description: null, is_system: true, name: "guest" }, true],
    [{ description: "Edited", is_system: true, name: "guest" }, false],
    [{ description: seeded, is_system: false, name: "guest" }, false],
    [{ description: "", is_system: true, name: "unknown" }, false],
  ])("hasDefaultRoleDescription(%j) -> %s", (role, expected) => {
    expect(hasDefaultRoleDescription(role)).toBe(expected);
  });

  it("translates untouched built-in descriptions only", () => {
    expect(
      roleDescription({ description: seeded, is_system: true, name: "guest" })
    ).toBe("role_guest_description");
    expect(
      roleDescription({ description: "Mine", is_system: true, name: "guest" })
    ).toBe("Mine");
    expect(
      roleDescription({ description: null, is_system: false, name: "ops" })
    ).toBeNull();
    expect(
      defaultRoleDescription({ is_system: false, name: "owner" })
    ).toBeUndefined();
  });
});

describe("permission labels", () => {
  it.each(ALL)("%s has a label and a hint", (permission) => {
    expect(permissionLabel(permission)).toMatch(/^permission_/u);
    expect(permissionHint(permission)).toMatch(/_hint$/u);
  });

  it("falls back to the code for unknown permissions", () => {
    expect(permissionLabel("new:thing")).toBe("new:thing");
    expect(permissionHint("new:thing")).toBeUndefined();
    expect(permissionGroupLabel("new")).toBe("new");
    expect(permissionGroupLabel("users")).toBe("permission_group_users");
  });

  it("flags exactly the hard-to-undo permissions as high risk", () => {
    expect([...HIGH_RISK_PERMISSIONS].toSorted()).toEqual([
      "roles:write",
      "settings:write",
      "shares:delete:any",
      "users:delete",
    ]);
  });
});
