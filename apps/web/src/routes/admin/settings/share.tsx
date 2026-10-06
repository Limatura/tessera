import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { ShareSettingsForm } from "./-components/share-settings-form";

export const Route = createFileRoute("/admin/settings/share")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: ShareSettingsRoute,
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
