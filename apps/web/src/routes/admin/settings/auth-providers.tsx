import { createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

import { OauthProviderCard } from "./-components/oauth-provider-card";
import { OidcProviderCard } from "./-components/oidc-provider-card";
import { ProviderRegistrationNote } from "./-components/provider-registration-note";
import { SettingsLoader } from "./-components/settings-loader";
import { SettingsPage } from "./-components/settings-page";

export const Route = createFileRoute("/admin/settings/auth-providers")({
  component: AuthProvidersRoute,
});

/**
 * 三個 provider 各一張卡、各自送出。
 *
 * 分開是因為 `PUT /admin/settings` 把一批 changes 一起驗證 —— 合成一個表單的話，
 * OIDC 的 issuer 打錯會讓同時改的 GitHub 設定也一起退回。
 */
function AuthProvidersRoute() {
  return (
    <SettingsPage
      description={m.settings_auth_providers_description()}
      title={m.settings_auth_providers_title()}
    >
      <SettingsLoader>
        {(settings) => (
          <>
            <ProviderRegistrationNote />
            <OidcProviderCard settings={settings} />
            <OauthProviderCard
              baseUrlHint={m.settings_auth_providers_github_base_url_hint()}
              description={m.settings_auth_providers_github_description()}
              provider="auth.github"
              scopesHint={m.settings_auth_providers_github_scopes_hint()}
              settings={settings}
              title={m.settings_auth_providers_github_title()}
            />
            <OauthProviderCard
              description={m.settings_auth_providers_discord_description()}
              provider="auth.discord"
              scopesHint={m.settings_auth_providers_discord_scopes_hint()}
              settings={settings}
              title={m.settings_auth_providers_discord_title()}
            />
          </>
        )}
      </SettingsLoader>
    </SettingsPage>
  );
}
