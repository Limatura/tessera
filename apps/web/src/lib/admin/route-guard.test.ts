import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

import type { QueryClient } from "@tanstack/react-query";
import { ADMIN_ENTRY_PERMISSIONS } from "@tessera/auth/permissions";
import { describe, expect, it, vi } from "vite-plus/test";

import type { Permission } from "./permissions";
import { landingRoute, PERMISSIONS } from "./permissions";
import { requirePermission } from "./route-guard";

vi.mock("@tessera/auth/hooks", () => ({
  sessionQueryOptions: { queryKey: ["auth", "session"] },
}));

function clientFor(permissions: string[] | null) {
  return {
    ensureQueryData: () =>
      permissions
        ? Promise.resolve({ permissions, roles: [] })
        : Promise.reject(new Error("401")),
  } as unknown as QueryClient;
}

/** `[permission, landing page, permission that page's route guards on]`. */
const LANDINGS = Object.values(PERMISSIONS).flatMap((permission) => {
  const to = landingRoute({ permissions: [permission], roles: [] } as never);
  return to === null ? [] : [[permission, to, permission] as const];
});

/**
 * Route path -> the permission its `beforeLoad` passes to `requirePermission`,
 * read from the route files' source. Parsing text rather than importing keeps
 * React, the router plugin and every page's dependencies out of this test.
 */
const guardsByRoute = new Map<string, Permission>();
const ROUTES_DIR = path.join(import.meta.dirname, "../../routes/admin");
const ROUTE_ID = /createFileRoute\("(?<id>[^"]+)"\)/u;
const GUARD = /requirePermission\([^)]*?PERMISSIONS\.(?<permission>\w+)/su;
for (const file of readdirSync(ROUTES_DIR, { recursive: true })) {
  if (!String(file).endsWith(".tsx")) {
    continue;
  }
  const source = readFileSync(path.join(ROUTES_DIR, String(file)), "utf-8");
  const id = ROUTE_ID.exec(source)?.groups?.id;
  const guard = GUARD.exec(source)?.groups?.permission;
  if (id && guard) {
    guardsByRoute.set(
      id.replace(/\/$/u, ""),
      PERMISSIONS[guard as keyof typeof PERMISSIONS]
    );
  }
}

async function redirectOf(guard: Promise<unknown>) {
  const thrown = await guard.then(
    () => null,
    (error: unknown) => error as { options: Record<string, unknown> }
  );
  if (thrown === null) {
    return null;
  }
  const { options } = thrown;
  // Drop router bookkeeping (status code) the guard does not choose.
  return options.search
    ? { search: options.search, to: options.to }
    : { to: options.to };
}

describe("requirePermission", () => {
  it("sends a signed-out visitor to sign in", async () => {
    expect(
      await redirectOf(
        requirePermission(
          clientFor(null),
          "/admin/users",
          PERMISSIONS.usersRead
        )
      )
    ).toEqual({ search: { redirect: "/admin/users" }, to: "/login" });
  });

  it("lets an account holding the permission through", async () => {
    const result = await requirePermission(
      clientFor(["users:read"]),
      "/admin/users",
      PERMISSIONS.usersRead
    );
    expect(result.account.permissions).toEqual(["users:read"]);
  });

  it.each([
    [["stats:read"], "/admin/users", PERMISSIONS.usersRead, "/admin/dashboard"],
    [
      ["settings:read"],
      "/admin/roles",
      PERMISSIONS.rolesRead,
      "/admin/settings",
    ],
    [
      ["invites:read", "settings:read"],
      "/admin/users",
      PERMISSIONS.usersRead,
      "/admin/invites",
    ],
  ])(
    "%j at %s without %s goes to %s",
    async (permissions, href, permission, to) => {
      expect(
        await redirectOf(
          requirePermission(clientFor(permissions), href, permission)
        )
      ).toEqual({ to });
    }
  );

  it.each([[[]], [["shares:create", "tokens:manage"]]])(
    "sends %j, which can open no admin page, to the site home",
    async (permissions) => {
      expect(
        await redirectOf(
          requirePermission(
            clientFor(permissions),
            "/admin/dashboard",
            PERMISSIONS.statsRead
          )
        )
      ).toEqual({ to: "/" });
    }
  );

  it.each(LANDINGS)(
    "lets an account holding only %s onto %s, query string and all",
    async (permission, to, guarded) => {
      await expect(
        requirePermission(clientFor([permission]), `${to}?q=1`, guarded)
      ).resolves.toBeDefined();
    }
  );
});

describe("landing pages", () => {
  it("covers every admin entry permission", () => {
    expect(LANDINGS.map(([permission]) => permission).toSorted()).toEqual(
      [...ADMIN_ENTRY_PERMISSIONS].toSorted()
    );
  });

  // A landing page whose route guarded on something else would bounce the
  // account between pages, so each must guard on its own permission.
  it.each(LANDINGS)("%s lands on %s, which guards on it", (permission, to) => {
    expect(guardsByRoute.get(to)).toBe(permission);
  });
});
