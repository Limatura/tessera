import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { FeatureSettingsForm } from "./-components/feature-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: FeatureSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/features",
});

function FeatureSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_features_description()}
      title={m.settings_features_title()}
    >
      <FeatureSettingsForm />
    </SettingsPage>
  );
}
