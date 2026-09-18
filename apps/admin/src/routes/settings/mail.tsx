import { createRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { MailSettingsForm } from "./-components/mail-settings-form";
import { SettingsPage } from "./-components/settings-page";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: MailSettingsRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/mail",
});

function MailSettingsRoute() {
  return (
    <SettingsPage
      description="SMTP relay、寄件位址，以及重設密碼與註冊確認連結的時效"
      title="寄信"
    >
      <MailSettingsForm />
    </SettingsPage>
  );
}
