import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { SiteSettingsForm } from "./-components/site-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: SiteSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/site",
});

function SiteSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_site_description()}
      title={m.settings_site_title()}
    >
      <SiteSettingsForm />
    </SettingsPage>
  );
}
