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
    // If the landing page is the one being requested, let it through, or two
    // beforeLoads would redirect forever.
    if (fallback !== href) {
      throw redirect({ to: fallback });
    }
  }

  return { account };
}
