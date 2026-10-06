import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { OgCardSettingsForm } from "./-components/og-card-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/og")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: OgCardSettingsRoute,
});

function OgCardSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_og_description()}
      title={m.settings_og_title()}
    >
      <OgCardSettingsForm />
    </SettingsPage>
  );
}
