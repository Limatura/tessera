import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { SettingsPage } from "./-components/settings-page";
import { UploadSettingsForm } from "./-components/upload-settings-form";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: UploadSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/upload",
});

function UploadSettingsRoute() {
  return (
    <SettingsPage
      description={m.settings_upload_description()}
      title={m.settings_upload_title()}
    >
      <UploadSettingsForm />
    </SettingsPage>
  );
}
