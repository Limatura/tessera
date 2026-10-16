import { Outlet, createFileRoute, useMatches } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";

import { AdminShell } from "@/components/admin/admin-shell";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

/**
 * The admin frame. The sidebar and title bar live here so only the content area
 * redraws on navigation; inside each page, every navigation would remount the sidebar
 * and lose the expanded settings group and the scroll position.
 *
 * The title bar's breadcrumb is every matched route with `staticData.title`, so a
 * settings tab reads "Settings › Mail". It stores a translation function, not a
 * string: `staticData` is fixed at module load, and a string would not follow a
 * language switch.
 *
 * This layer only requires sign-in, since the sidebar reads the session; each page
 * still checks its own permissions in `beforeLoad`. `reducedMotion="user"` turns off
 * the panel's motion when the system asks for reduced motion.
 */
export const Route = createFileRoute("/admin")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: AdminLayout,
  head: () => ({
    meta: [
      {
        title: m.admin_app_name(),
      },
      {
        content: m.admin_app_description(),
        name: "description",
      },
    ],
  }),
});

function AdminLayout() {
  const titled = useMatches({
    select: (matches) =>
      matches.flatMap((match) =>
        match.staticData.title
          ? [{ title: match.staticData.title, to: match.pathname }]
          : []
      ),
  });
  const crumbs = titled.map((crumb) => ({ ...crumb, title: crumb.title() }));

  return (
    <MotionConfig reducedMotion="user">
      <AdminShell
        crumbs={
          crumbs.length > 0
            ? crumbs
            : [{ title: m.admin_app_name(), to: "/admin" }]
        }
      >
        <Outlet />
      </AdminShell>
    </MotionConfig>
  );
}
