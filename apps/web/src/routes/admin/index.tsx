import { createFileRoute, redirect } from "@tanstack/react-router";

import { landingRoute } from "@/lib/admin/permissions";
import { requireAuth } from "@/lib/route-guard";

/**
 * The admin index shows nothing itself; it only sends the user to the first page their
 * permissions allow.
 *
 * With few permissions the dashboard would be empty, so after sign-in this finds the
 * first reachable page (see LANDING_ORDER in lib/admin/permissions.ts); with none, the
 * user goes back to the public site.
 */
export const Route = createFileRoute("/admin/")({
  beforeLoad: async ({ context, location }) => {
    const { account } = await requireAuth(context.queryClient, location.href);
    const to = landingRoute(account);
    if (to === null) {
      throw redirect({ to: "/" });
    }
    throw redirect({ to });
  },
});
