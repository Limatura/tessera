import { IconPlus } from "@tabler/icons-react";
import { useSession } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import { Separator } from "@tessera/ui/components/separator";
import { useState } from "react";

import { CreateWebhookDialog } from "@/components/webhooks/create-webhook-dialog";
import { WebhookList } from "@/components/webhooks/webhook-list";
import { accountWebhooks } from "@/hooks/use-webhooks";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

import { Section } from "./section";

/** An account's webhooks hear about its own shares, so they need `shares:read`. */
export function WebhooksSection() {
  const { data: account } = useSession();
  const [creating, setCreating] = useState(false);

  if (!hasPermission(account, PERMISSIONS.sharesRead)) {
    return null;
  }

  // The separator is in here so an account without `shares:read` does not get two in a row.
  return (
    <>
      <Section
        action={
          <Button
            onClick={() => setCreating(true)}
            size="sm"
            type="button"
            variant="outline"
          >
            <IconPlus size={16} />
            {m.webhooks_new()}
          </Button>
        }
        description={m.webhooks_account_description()}
        title={m.webhooks_title()}
      >
        <WebhookList source={accountWebhooks} canRemove />
        <CreateWebhookDialog
          source={accountWebhooks}
          onOpenChange={setCreating}
          open={creating}
        />
      </Section>
      <Separator />
    </>
  );
}
