import type { QueryClient } from "@tanstack/react-query";
import { redirect } from "@tanstack/react-router";

import { requireAuth } from "@/lib/route-guard";

import type { Permission } from "./permissions";
import { hasPermission, landingRoute } from "./permissions";

/**
 * Admin pages that need a specific permission.
 *
 * Signed-out users are sent to sign in by `requireAuth`; a forced password
 * change is stopped earlier by the root's `requirePasswordChanged`.
 *
 * Without the permission, redirect to the user's own first accessible page
 * instead of showing a 403: they most likely followed an old bookmark or a
 * pasted link, and leaving them on a URL they cannot open helps no one. The
 * server does the real authorisation; this only avoids a blank screen.
 */
export async function requirePermission(
  queryClient: QueryClient,
  href: string,
  permission: Permission
) {
  const { account } = await requireAuth(queryClient, href);

  if (!hasPermission(account, permission)) {
    const fallback = landingRoute(account);
    // No admin page is accessible: this account does not belong in the admin
    // area, so send it to the site home.
    if (fallback === null) {
      throw redirect({ to: "/" });
    }
    // Defensive only: `landingRoute` returns a page the account can open, and
    // each landing page guards on the permission `LANDING_ORDER` lists for it
    // (route-guard.test.ts checks this), so today a request never lacks the
    // permission of its own landing page. Should a page ever guard on
    // something else, this lets it through instead of redirecting to itself.
    // The path is compared alone so a query string would not defeat it.
    if (fallback !== href.split(/[?#]/u)[0]) {
      throw redirect({ to: fallback });
    }
  }

  return { account };
}
