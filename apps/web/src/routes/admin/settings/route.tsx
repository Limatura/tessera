import {
  Outlet,
  createFileRoute,
  useRouterState,
} from "@tanstack/react-router";

import { PageTransition } from "@/components/page-transition";
import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { SettingsNav } from "./-components/settings-nav";

/**
 * Layout shared by every settings tab: grouped menu on the left, the current
 * tab on the right. The sidebar frame is one level up in `admin/route.tsx`.
 *
 * Permissions are checked here once, since every settings tab needs only
 * `settings:read`, and they share the "Settings" title.
 */
export const Route = createFileRoute("/admin/settings")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: SettingsLayout,
  staticData: { title: m.settings_page_shell_title },
});

/**
 * Menu and content are width-limited and centred together. Limiting only the
 * content and aligning left leaves a wide empty strip on large screens. Every
 * tab uses the same width so the menu does not jump between tabs.
 */
function SettingsLayout() {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });

  return (
    <div className="mx-auto grid w-full max-w-6xl gap-6 lg:grid-cols-[12rem_minmax(0,1fr)] lg:gap-10">
      <SettingsNav />
      <PageTransition className="min-w-0" transitionKey={pathname}>
        <Outlet />
      </PageTransition>
    </div>
  );
}
