import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { SettingsCard } from "./-components/settings-card";
import { UploadSettingsForm } from "./-components/upload-settings-form";

export const Route = createFileRoute("/admin/settings/upload")({
  component: UploadSettingsRoute,
});

function UploadSettingsRoute() {
  return (
    <SettingsCard
      description={m.settings_upload_description()}
      title={m.settings_upload_title()}
    >
      <UploadSettingsForm />
    </SettingsCard>
  );
}
