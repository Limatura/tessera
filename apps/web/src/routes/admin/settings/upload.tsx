import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsPage } from "./-components/settings-page";
import { UploadSettingsForm } from "./-components/upload-settings-form";

export const Route = createFileRoute("/admin/settings/upload")({
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
