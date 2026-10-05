import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { UploadSettingsForm } from "./-components/upload-settings-form";

export const Route = createFileRoute("/admin/settings/upload")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: UploadSettingsRoute,
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
