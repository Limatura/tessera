import type { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vite-plus/test";

import { PERMISSIONS } from "./permissions";
import { requirePermission } from "./route-guard";

vi.mock("@tessera/auth/hooks", () => ({
  sessionQueryOptions: { queryKey: ["auth", "session"] },
}));

// The `@/` alias is not set up for unit tests; point it at the real guard.
vi.mock("@/lib/route-guard", () => import("../route-guard"));

const { isMessage } = vi.hoisted(() => ({
  isMessage: (key: string | symbol): key is string =>
    typeof key === "string" &&
    (key.startsWith("role_") || key.startsWith("permission_")),
}));

// permissions.ts builds its label tables from messages; none are read here.
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

function clientFor(permissions: string[] | null) {
  return {
    ensureQueryData: () =>
      permissions
        ? Promise.resolve({ permissions, roles: [] })
        : Promise.reject(new Error("401")),
  } as unknown as QueryClient;
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
      "/admin/settings",
      PERMISSIONS.settingsWrite,
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

  it("does not loop when the landing page itself is requested", async () => {
    await expect(
      requirePermission(
        clientFor(["users:read"]),
        "/admin/users",
        PERMISSIONS.usersWrite
      )
    ).resolves.toBeDefined();
  });

  it("keeps a query string on the landing page instead of redirecting", async () => {
    await expect(
      requirePermission(
        clientFor(["users:read"]),
        "/admin/users?q=ada",
        PERMISSIONS.usersWrite
      )
    ).resolves.toBeDefined();
  });

  it("only redirects to internal admin paths", async () => {
    const target = await redirectOf(
      requirePermission(
        clientFor(["roles:read"]),
        "https://evil.example/admin",
        PERMISSIONS.usersRead
      )
    );
    expect(target).toEqual({ to: "/admin/roles" });
  });
});
