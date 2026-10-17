import type { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vite-plus/test";

import {
  CHANGE_PASSWORD_PATH,
  requireAuth,
  requirePasswordChanged,
} from "./route-guard";

// The real options pull in the API client; the guard only needs a key.
vi.mock("@tessera/auth/hooks", () => ({
  sessionQueryOptions: { queryKey: ["auth", "session"] },
}));

interface FakeCache {
  account?: Record<string, unknown>;
  status?: "error" | "pending" | "success";
}

function fakeClient({ account, status }: FakeCache = {}) {
  const ensureQueryData = vi.fn<() => Promise<unknown>>(() =>
    account ? Promise.resolve(account) : Promise.reject(new Error("401"))
  );
  const getQueryState = vi.fn<() => { status: string } | undefined>(() =>
    status ? { status } : undefined
  );
  const client = { ensureQueryData, getQueryState } as unknown as QueryClient;
  return { client, ensureQueryData };
}

/** Runs a guard and returns the redirect options it threw, or `null`. */
async function redirectOf(guard: Promise<unknown>) {
  const thrown = await guard.then(
    () => null,
    (error: unknown) => error as { options: Record<string, unknown> }
  );
  if (thrown === null) {
    return null;
  }
  const { options } = thrown;
  // Drop router bookkeeping (status code) the guards do not choose.
  return { search: options.search, to: options.to };
}

describe("requireAuth", () => {
  it("returns the signed-in account", async () => {
    const account = { id: "a" };
    const { client } = fakeClient({ account });
    await expect(requireAuth(client, "/admin/users")).resolves.toEqual({
      account,
    });
  });

  it("sends a signed-out visitor to sign in, remembering where they were", async () => {
    const { client } = fakeClient();
    expect(await redirectOf(requireAuth(client, "/admin/users?p=2"))).toEqual({
      search: { redirect: "/admin/users?p=2" },
      to: "/login",
    });
  });
});

describe("requirePasswordChanged", () => {
  it("never redirects away from the change password page", async () => {
    const { client, ensureQueryData } = fakeClient({
      account: { must_change_password: true },
    });
    expect(
      await redirectOf(requirePasswordChanged(client, at(CHANGE_PASSWORD_PATH)))
    ).toBeNull();
    expect(ensureQueryData).not.toHaveBeenCalled();
  });

  it("skips the session request when the cache already says signed out", async () => {
    const { client, ensureQueryData } = fakeClient({ status: "error" });
    expect(
      await redirectOf(requirePasswordChanged(client, at("/")))
    ).toBeNull();
    expect(ensureQueryData).not.toHaveBeenCalled();
  });

  it("lets an anonymous visitor through", async () => {
    const { client } = fakeClient();
    expect(
      await redirectOf(requirePasswordChanged(client, at("/s/abc")))
    ).toBeNull();
  });

  it("lets an account without a pending change through", async () => {
    const { client } = fakeClient({
      account: { must_change_password: false },
      status: "success",
    });
    expect(
      await redirectOf(requirePasswordChanged(client, at("/admin")))
    ).toBeNull();
  });

  it("holds an account that must change its password", async () => {
    const { client } = fakeClient({ account: { must_change_password: true } });
    expect(
      await redirectOf(requirePasswordChanged(client, at("/admin/users?q=1")))
    ).toEqual({
      search: { redirect: "/admin/users?q=1" },
      to: CHANGE_PASSWORD_PATH,
    });
  });
});

function at(href: string) {
  return { href, pathname: href.split("?")[0] ?? "" };
}
