import { IconKey, IconPlus, IconTrash } from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { ApiToken } from "@/hooks/use-tokens";
import { useRevokeToken, useTokens } from "@/hooks/use-tokens";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { CreateApiKeyDialogBody } from "./create-api-key-dialog";
import { EmptyBox, Section } from "./section";

export function ApiKeysSection() {
  const tokens = useTokens();
  const revoke = useRevokeToken();
  const formatDate = useDateTimeFormatter();
  const [creating, setCreating] = useState(false);

  const handleRevoke = (token: ApiToken) => {
    revoke.mutate(token.id, {
      onError: (error) =>
        toast.add({
          description: apiErrorMessage(error, m.common_try_again()),
          title: m.user_api_keys_revoke_failed(),
          type: "error",
        }),
      onSuccess: () =>
        toast.add({ title: m.user_api_keys_revoked(), type: "success" }),
    });
  };

  const renderBody = () => {
    if (tokens.isPending) {
      return <Skeleton className="h-16 w-full" />;
    }
    if (tokens.isError) {
      return (
        <p className="text-destructive text-sm">{m.user_api_keys_error()}</p>
      );
    }
    const active = tokens.data.filter((token) => !token.revoked_at);
    if (active.length === 0) {
      return (
        <EmptyBox icon={<IconKey size={20} />}>
          {m.user_api_keys_empty()}
        </EmptyBox>
      );
    }
    return (
      <ul className="space-y-3">
        {active.map((token) => (
          <ApiKeyRow
            formatDate={formatDate}
            isRevoking={revoke.isPending && revoke.variables === token.id}
            key={token.id}
            onRevoke={handleRevoke}
            token={token}
          />
        ))}
      </ul>
    );
  };

  return (
    <Section
      action={
        <Button
          onClick={() => setCreating(true)}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconPlus size={16} />
          {m.user_api_keys_new()}
        </Button>
      }
      description={m.user_api_keys_description()}
      title={m.user_api_keys_title()}
    >
      {renderBody()}
      <ResponsiveDialog onOpenChange={setCreating} open={creating}>
        <ResponsiveDialogContent>
          {creating ? (
            <CreateApiKeyDialogBody onDone={() => setCreating(false)} />
          ) : null}
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </Section>
  );
}

function ApiKeyRow({
  token,
  formatDate,
  isRevoking,
  onRevoke,
}: {
  token: ApiToken;
  formatDate: (value: string | null | undefined) => string;
  isRevoking: boolean;
  onRevoke: (token: ApiToken) => void;
}) {
  const [confirming, setConfirming] = useState(false);

  return (
    <li className="flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3">
      <span className="mt-0.5 shrink-0 text-muted-foreground">
        <IconKey size={18} />
      </span>
      <div className="min-w-0 flex-1 basis-48 space-y-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="break-all font-medium text-sm">{token.name}</span>
          <span className="font-mono text-muted-foreground text-xs">
            {token.display_prefix}…
          </span>
        </div>
        <div className="flex flex-wrap gap-1">
          {token.permissions.map((permission) => (
            <Badge key={permission} variant="secondary">
              {permission}
            </Badge>
          ))}
        </div>
        <p className="text-muted-foreground text-xs">
          {m.user_api_keys_created_at({ date: formatDate(token.created_at) })}
          {" · "}
          {token.last_used_at
            ? m.user_api_keys_last_used({
                date: formatDate(token.last_used_at),
              })
            : m.user_api_keys_never_used()}
          {" · "}
          {token.expires_at
            ? m.user_api_keys_expires_at({ date: formatDate(token.expires_at) })
            : m.common_never_expires()}
        </p>
      </div>
      <Button
        className="w-full sm:w-auto"
        disabled={isRevoking}
        onClick={() => setConfirming(true)}
        size="sm"
        type="button"
        variant="outline"
      >
        <IconTrash size={16} />
        {m.user_api_keys_revoke()}
      </Button>
      <ResponsiveDialog alert onOpenChange={setConfirming} open={confirming}>
        <ResponsiveDialogContent>
          <ResponsiveDialogHeader>
            <ResponsiveDialogTitle>
              {m.user_api_keys_revoke_confirm_title()}
            </ResponsiveDialogTitle>
            <ResponsiveDialogDescription>
              {m.user_api_keys_revoke_confirm_description()}
            </ResponsiveDialogDescription>
          </ResponsiveDialogHeader>
          <ResponsiveDialogFooter>
            <Button
              onClick={() => setConfirming(false)}
              type="button"
              variant="outline"
            >
              {m.common_cancel()}
            </Button>
            <Button
              onClick={() => {
                setConfirming(false);
                onRevoke(token);
              }}
              type="button"
              variant="destructive"
            >
              {m.user_api_keys_revoke()}
            </Button>
          </ResponsiveDialogFooter>
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </li>
  );
}
