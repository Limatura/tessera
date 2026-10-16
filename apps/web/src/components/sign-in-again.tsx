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

/**
 * Where to land after signing in again. The account page opens on the account
 * tab by default, so no search params are needed.
 */
const RETURN_TO = "/user";

/**
 * "Please sign in again to confirm it is you", with a button to do so.
 *
 * The server wants proof of a sign-in within the last 10 minutes, so the remedy
 * is to sign in again, come back and press the button again:
 *
 * - With a password: sign out, go to the sign-in page, and come back to the
 *   account page through `?redirect=`.
 * - Without one: there is no sign-in page to use, so go through a linked
 *   provider again with `return_to`. With several linked, each gets a button,
 *   since any of them proves identity.
 *
 * What was typed is not kept: the page is replaced and must be filled in
 * again afterwards.
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
              // Navigate instead of fetch: the server answers 303 and the
              // browser must follow it to the provider.
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
