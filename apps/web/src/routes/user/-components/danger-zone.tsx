import { IconAlertCircle, IconLogout, IconRefresh } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import type * as React from "react";

import { useLogout } from "@/hooks/use-auth";
import * as m from "@/paraglide/messages";

import { notifyComingSoon } from "./coming-soon";

export function DangerZone() {
  const logout = useLogout();

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
              onClick={notifyComingSoon}
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
              onClick={() => logout.mutate()}
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
