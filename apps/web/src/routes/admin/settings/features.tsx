import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { FeatureSettingsForm } from "./-components/feature-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/features")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: FeatureSettingsRoute,
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
