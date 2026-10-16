import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { ProviderList } from "./-components/provider-list";
import { SettingsLoader } from "./-components/settings-loader";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/auth-providers")({
  component: AuthProvidersRoute,
  staticData: { title: m.nav_settings_auth_providers },
});

/**
 * One row per provider, each opening its own form in a side panel and submitted on its own.
 *
 * `PUT /admin/settings` validates a batch of changes together, so in one form a
 * typo in the OIDC issuer would also send back a GitHub change made alongside.
 */
function AuthProvidersRoute() {
  return (
    <SettingsPage
      description={m.settings_auth_providers_description()}
      title={m.settings_auth_providers_title()}
    >
      <SettingsLoader>
        {(settings) => <ProviderList settings={settings} />}
      </SettingsLoader>
    </SettingsPage>
  );
}
