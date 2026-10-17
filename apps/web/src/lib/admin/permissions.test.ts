import { describe, expect, it, vi } from "vite-plus/test";

import type { Account } from "./permissions";
import {
  accountRoleLabel,
  canAccessAdmin,
  defaultRoleDescription,
  hasDefaultRoleDescription,
  HIGH_RISK_PERMISSIONS,
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
