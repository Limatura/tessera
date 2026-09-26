import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { ShareSettingsForm } from "./-components/share-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: ShareSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/share",
});

function ShareSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_share_description()}
      title={m.settings_share_title()}
    >
      <ShareSettingsForm />
    </SettingsPage>
  );
}
