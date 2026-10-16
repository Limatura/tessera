import { IconLink, IconUnlink } from "@tabler/icons-react";
import { getRouteApi } from "@tanstack/react-router";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";
import type * as React from "react";

import { ProviderIcon } from "@/components/provider-icon";
import { SignInAgain } from "@/components/sign-in-again";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { Identity } from "@/hooks/use-identity-providers";
import {
  useIdentities,
  useProviders,
  useUnlinkIdentity,
} from "@/hooks/use-identity-providers";
import { apiErrorMessage } from "@/lib/api-error";
import { oidcErrorMessage, startLink } from "@/lib/api/identity-provider";
import * as m from "@/paraglide/messages";

import { EmptyBox, Section } from "./section";

/** Importing Route directly would create an import cycle with index.tsx (as in login-form). */
const routeApi = getRouteApi("/_site/user/");

/**
 * 409 means this is the last way in (`last_sign_in_method`). The backend decides, since
 * the client does not know whether the account has a password.
 */
function errorMessage(error: unknown): string {
  return apiErrorMessage(error, m.common_try_again());
}

/**
 * One row per provider, linked or not. This section answers "which ways can I sign in":
 * showing only linked ones would leave a never-linked user an empty box and no idea
 * what can be linked. An account can link several providers, so it is a list, not a
 * single choice.
 */
export function IdentitiesSection() {
  const providers = useProviders();
  const identities = useIdentities();
  const unlink = useUnlinkIdentity();
  const formatDate = useDateTimeFormatter();

  const handleUnlink = (provider: string) => {
    unlink.mutate(provider, {
      onError: (error) => {
        toast.add({
          description: errorMessage(error),
          title: m.user_identities_unlink_failed(),
          type: "error",
        });
      },
      onSuccess: () =>
        toast.add({ title: m.user_identities_unlink_done(), type: "success" }),
    });
  };

  if (providers.isPending || identities.isPending) {
    return (
      <IdentitiesShell>
        <div className="space-y-3">
          <Skeleton className="h-16 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      </IdentitiesShell>
    );
  }

  if (providers.isError || identities.isError) {
    return (
      <IdentitiesShell>
        <p className="text-destructive text-sm">{m.user_identities_error()}</p>
      </IdentitiesShell>
    );
  }

  const enabled = providers.data?.filter((provider) => provider.enabled) ?? [];
  const linked = identities.data ?? [];

  /*
   * When the instance disables a provider, linked identities stay in the database and
   * still count in is_last_way_in. Hidden, one would look never linked yet be impossible
   * to remove, so they stay listed, just without a link button.
   */
  const orphaned = linked.filter(
    (identity) =>
      !enabled.some((provider) => provider.provider === identity.provider)
  );

  if (enabled.length === 0 && orphaned.length === 0) {
    return (
      <IdentitiesShell>
        <EmptyBox icon={<IconLink size={20} />}>
          {m.user_identities_none_configured()}
        </EmptyBox>
      </IdentitiesShell>
    );
  }

  return (
    <IdentitiesShell>
      <ul className="space-y-3">
        {enabled.map((provider) => (
          <IdentityRow
            displayName={provider.display_name}
            formatDate={formatDate}
            identity={linked.find(
              (item) => item.provider === provider.provider
            )}
            isBusy={unlink.isPending && unlink.variables === provider.provider}
            key={provider.provider}
            onUnlink={handleUnlink}
            provider={provider.provider}
          />
        ))}
        {orphaned.map((identity) => (
          <IdentityRow
            // Once disabled the backend stops returning display_name; only the id is left.
            displayName={identity.provider}
            formatDate={formatDate}
            identity={identity}
            isBusy={unlink.isPending && unlink.variables === identity.provider}
            isRetired
            key={identity.provider}
            onUnlink={handleUnlink}
            provider={identity.provider}
          />
        ))}
      </ul>
    </IdentitiesShell>
  );
}

/**
 * Title, description and error message, factored out so all three return branches
 * show the message.
 *
 * A failed link sends the user back here with a code. It shows in this section, not at
 * the top of the page, to stay close to the button just pressed; unknown codes fall
 * back to a generic message.
 *
 * `sign_in_again` is not a failure: linking needs a sign-in within 10 minutes, and the
 * fix is signing in again, so it gets a button instead of red text. Linking again after
 * coming back succeeds.
 */
function IdentitiesShell({ children }: { children: React.ReactNode }) {
  const { oidc_error: code } = routeApi.useSearch();
  const error =
    code && code !== "sign_in_again"
      ? (oidcErrorMessage(code) ?? m.oidc_error_generic())
      : null;

  return (
    <Section
      description={m.user_identities_description()}
      title={m.user_identities_title()}
    >
      {code === "sign_in_again" ? (
        <SignInAgain message={m.oidc_error_sign_in_again()} />
      ) : null}
      {error ? (
        <p
          className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-destructive text-sm"
          role="alert"
        >
          {error}
        </p>
      ) : null}
      {children}
    </Section>
  );
}

function IdentityRow({
  provider,
  displayName,
  identity,
  formatDate,
  onUnlink,
  isBusy,
  isRetired = false,
}: {
  provider: string;
  displayName: string;
  identity: Identity | undefined;
  formatDate: (value: string | null | undefined) => string;
  onUnlink: (provider: string) => void;
  isBusy: boolean;
  isRetired?: boolean;
}) {
  const isLastWayIn = identity?.is_last_way_in ?? false;

  return (
    /* flex-wrap as in session-row: full-width button on its own line on phones,
       inline on desktop. */
    <li className="flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3">
      <span className="mt-0.5 shrink-0 text-muted-foreground">
        <ProviderIcon provider={provider} size={18} />
      </span>

      <div className="min-w-0 flex-1 basis-48 space-y-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="font-medium text-sm">{displayName}</span>
          {isRetired ? (
            <Badge variant="outline">{m.user_identities_retired()}</Badge>
          ) : null}
        </div>
        <p className="text-muted-foreground text-xs">
          {identity
            ? m.user_identities_linked_at({
                date: formatDate(identity.created_at),
              })
            : m.user_identities_not_linked()}
        </p>
        {/* The lock reason is written beside the button, not left to `disabled`: a disabled
            button gets no hover, so a touch device could never open its tooltip. */}
        {isLastWayIn ? (
          <p className="text-muted-foreground text-xs">
            {m.user_identities_last_way_in()}
          </p>
        ) : null}
      </div>

      {identity ? (
        <Button
          className="w-full sm:w-auto"
          disabled={isBusy || isLastWayIn}
          onClick={() => onUnlink(provider)}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconUnlink size={16} />
          {m.user_identities_unlink()}
        </Button>
      ) : (
        <Button
          className="w-full sm:w-auto"
          // Navigate instead of fetch: the backend answers 303 and the browser
          // must follow it to the provider.
          onClick={() => startLink(provider)}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconLink size={16} />
          {m.user_identities_link()}
        </Button>
      )}
    </li>
  );
}
