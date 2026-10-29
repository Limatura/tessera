import { IconPlus } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { useState } from "react";

import { CreateWebhookDialog } from "@/components/webhooks/create-webhook-dialog";
import { WebhookList } from "@/components/webhooks/webhook-list";
import { instanceWebhooks } from "@/hooks/admin/use-webhooks";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

export const Route = createFileRoute("/admin/webhooks/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: WebhooksComponent,
  staticData: { title: m.webhooks_page_title },
});

/** Looking needs `settings:read`; adding and removing need `settings:write`. */
function WebhooksComponent() {
  const { data: account } = useSession();
  const [creating, setCreating] = useState(false);
  const canWrite = hasPermission(account, PERMISSIONS.settingsWrite);

  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.webhooks_title()}</CardTitle>
        <CardDescription>{m.webhooks_instance_description()}</CardDescription>
        {canWrite && (
          <CardAction>
            <Button onClick={() => setCreating(true)} type="button">
              <IconPlus />
              {m.webhooks_new()}
            </Button>
          </CardAction>
        )}
      </CardHeader>
      <CardContent>
        <WebhookList source={instanceWebhooks} canRemove={canWrite} />
      </CardContent>
      {canWrite && (
        <CreateWebhookDialog
          source={instanceWebhooks}
          onOpenChange={setCreating}
          open={creating}
        />
      )}
    </Card>
  );
}
