import { IconAlertCircle, IconLogout, IconRefresh } from "@tabler/icons-react";
import { useNavigate } from "@tanstack/react-router";
import { useLogout } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import { toast } from "@tessera/ui/components/toast";
import type * as React from "react";

import { useRevokeOtherSessions } from "@/hooks/use-sessions";
import * as m from "@/paraglide/messages";

import { DeleteAccountButton } from "./delete-account-dialog";

export function DangerZone() {
  const logout = useLogout();
  const navigate = useNavigate();
  const revokeOthers = useRevokeOtherSessions();

  const handleRevokeOthers = () => {
    revokeOthers.mutate(undefined, {
      onError: () =>
        toast.add({
          description: m.common_try_again(),
          title: m.user_sessions_revoke_failed(),
          type: "error",
        }),
      onSuccess: () =>
        toast.add({
          title: m.user_sessions_revoke_others_done(),
          type: "success",
        }),
    });
  };

  return (
    <section className="space-y-4">
      <div className="flex items-start gap-2">
        <IconAlertCircle className="mt-0.5 text-destructive" size={18} />
        <div>
          <h2 className="font-medium text-base text-destructive">
            {m.user_danger_title()}
          </h2>
          <p className="text-muted-foreground text-sm">
            {m.user_danger_description()}
          </p>
        </div>
      </div>

      <div className="grid gap-6 rounded-lg border border-destructive/40 bg-destructive/5 p-4 md:grid-cols-2">
        <DangerAction
          action={
            <Button
              disabled={revokeOthers.isPending}
              onClick={handleRevokeOthers}
              size="sm"
              type="button"
              variant="outline"
            >
              <IconRefresh size={16} />
              {m.user_danger_revoke_action()}
            </Button>
          }
          description={m.user_danger_revoke_description()}
          title={m.user_danger_revoke_title()}
        />
        <DangerAction
          action={
            <Button
              disabled={logout.isPending}
              onClick={() =>
                // 登出後回首頁,跟刪除帳號一樣(#363)。
                logout.mutate(undefined, {
                  onSuccess: () => navigate({ to: "/" }),
                })
              }
              size="sm"
              type="button"
              variant="destructive"
            >
              <IconLogout size={16} />
              {m.user_danger_logout_action()}
            </Button>
          }
          description={m.user_danger_logout_description()}
          title={m.user_danger_logout_title()}
        />
        <DangerAction
          action={<DeleteAccountButton />}
          description={m.user_danger_delete_description()}
          title={m.user_danger_delete_title()}
        />
      </div>
    </section>
  );
}

function DangerAction({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action: React.ReactNode;
}) {
  return (
    <div className="space-y-2">
      <p className="font-medium text-sm">{title}</p>
      <p className="text-muted-foreground text-sm">{description}</p>
      {action}
    </div>
  );
}
