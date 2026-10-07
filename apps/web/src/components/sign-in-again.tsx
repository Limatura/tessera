import { IconLogin2 } from "@tabler/icons-react";
import { useNavigate } from "@tanstack/react-router";
import { useLogout } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";

import { ProviderIcon } from "@/components/provider-icon";
import { useHasPassword } from "@/hooks/use-account";
import {
  useEnabledProviders,
  useIdentities,
} from "@/hooks/use-identity-providers";
import { startSignIn } from "@/lib/api/identity-provider";
import * as m from "@/paraglide/messages";

/** 重新登入後落地的地方。帳號頁的分頁預設就是「帳號」，不用多帶 search。 */
const RETURN_TO = "/user";

/**
 * 「請重新登入確認是你本人」，附上重新登入的按鈕（#434、#435）。
 *
 * 後端要的證明是「10 分鐘內登入過」，所以補救就是再登入一次，回來後再按一次：
 *
 * - 有密碼：登出、回登入頁，登入後經 `?redirect=` 回到帳號頁。
 * - 沒密碼：沒有登入頁可回，直接重走帳號綁定的第三方，帶 `return_to` 回來。綁了
 *   好幾家就每家一顆，哪家都能證明是本人。
 *
 * 剛才填的東西不會留著 —— 整頁會換掉，回來後重新填一次。
 */
export function SignInAgain({ message }: { message: string }) {
  const hasPassword = useHasPassword();
  const navigate = useNavigate();
  const logout = useLogout();
  const providers = useEnabledProviders();
  const identities = useIdentities();

  const linked = providers.filter((provider) =>
    identities.data?.some((identity) => identity.provider === provider.provider)
  );

  const handleSignInAgain = () => {
    logout.mutate(undefined, {
      onSuccess: () =>
        navigate({ search: { redirect: RETURN_TO }, to: "/login" }),
    });
  };

  return (
    <div
      className="space-y-3 rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
      role="alert"
    >
      <p>{message}</p>
      <div className="flex flex-wrap gap-2">
        {hasPassword ? (
          <Button
            disabled={logout.isPending}
            onClick={handleSignInAgain}
            size="sm"
            type="button"
            variant="outline"
          >
            <IconLogin2 size={16} />
            {m.user_reauth_sign_in_again()}
          </Button>
        ) : (
          linked.map((provider) => (
            <Button
              key={provider.provider}
              // 導頁而不是 fetch：後端回 303，要瀏覽器自己跟到 provider 去。
              onClick={() =>
                startSignIn(provider.provider, { returnTo: RETURN_TO })
              }
              size="sm"
              type="button"
              variant="outline"
            >
              <ProviderIcon provider={provider.provider} size={16} />
              {m.auth_continue_with({ provider: provider.display_name })}
            </Button>
          ))
        )}
      </div>
    </div>
  );
}
