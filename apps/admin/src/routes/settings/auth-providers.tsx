import { createRoute } from "@tanstack/react-router";

import { AdminShell } from "@/components/admin-shell";
import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { OauthProviderCard } from "./-components/oauth-provider-card";
import { OidcProviderCard } from "./-components/oidc-provider-card";
import { SettingsLoader } from "./-components/settings-loader";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: AuthProvidersRoute,
  getParentRoute: () => rootRoute,
  path: "/settings/auth-providers",
});

/**
 * 三個 provider 各一張卡、各自送出。
 *
 * 分開是因為 `PUT /admin/settings` 把一批 changes 一起驗證 —— 合成一個表單的話，
 * OIDC 的 issuer 打錯會讓同時改的 GitHub 設定也一起退回。
 */
function AuthProvidersRoute() {
  return (
    <AdminShell title="系統設定">
      <SettingsLoader>
        {(settings) => (
          <>
            <OidcProviderCard settings={settings} />
            <OauthProviderCard
              baseUrlHint="給 GitHub Enterprise Server 用（自架在自己的網域下）。留空代表 github.com。"
              description="GitHub 不是 OIDC provider（不發 id_token），端點寫死在 adapter 裡，所以這裡只有向 GitHub 表明本站身分的那組值。"
              provider="auth.github"
              scopesHint="以空白分隔。沒有 user:email 的話 GitHub 不給已驗證信箱，每次登入都會以 no_address 失敗。"
              settings={settings}
              title="GitHub"
            />
            <OauthProviderCard
              description="Discord 同樣不是 OIDC provider。沒有自架版本，所以沒有自訂網域可以設。"
              provider="auth.discord"
              scopesHint="以空白分隔。沒有 email 的話 Discord 不給信箱，每次登入都會以 no_address 失敗。"
              settings={settings}
              title="Discord"
            />
          </>
        )}
      </SettingsLoader>
    </AdminShell>
  );
}
