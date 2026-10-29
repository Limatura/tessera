import { IconHistory, IconTrash, IconWebhook } from "@tabler/icons-react";
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
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { DeliveriesDialog } from "./deliveries-dialog";
import { eventLabel } from "./labels";
import type { Webhook, WebhooksSource } from "./types";
import { useRemoveWebhook, useWebhookList } from "./use-webhooks";

/**
 * The list of webhooks with their delivery log and remove button. Adding is
 * the caller's: it sits in the page's own header.
 *
 * `canRemove` is false for someone who may look but not change; the server
 * refuses the call either way.
 */
export function WebhookList({
  source,
  canRemove,
}: {
  source: WebhooksSource;
  canRemove: boolean;
}) {
  const webhooks = useWebhookList(source);
  const remove = useRemoveWebhook(source);
  const [viewing, setViewing] = useState<Webhook | null>(null);
  const [removing, setRemoving] = useState<Webhook | null>(null);

  const handleRemove = (webhook: Webhook) => {
    remove.mutate(webhook.id, {
      onError: (error) =>
        toast.add({
          description: apiErrorMessage(error, m.common_try_again()),
          title: m.webhooks_remove_failed(),
          type: "error",
        }),
      onSuccess: () =>
        toast.add({ title: m.webhooks_removed(), type: "success" }),
    });
  };

  if (webhooks.isPending) {
    return <Skeleton className="h-16 w-full" />;
  }
  if (webhooks.isError) {
    return (
      <p className="text-destructive text-sm" role="alert">
        {m.webhooks_error()}
      </p>
    );
  }
  if (webhooks.data.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-6 py-10 text-muted-foreground text-sm">
        <IconWebhook size={20} />
        {m.webhooks_empty()}
      </div>
    );
  }

  return (
    <>
      <ul className="space-y-3">
        {webhooks.data.map((webhook) => (
          <WebhookRow
            canRemove={canRemove}
            isRemoving={remove.isPending && remove.variables === webhook.id}
            key={webhook.id}
            onRemove={() => setRemoving(webhook)}
            onViewDeliveries={() => setViewing(webhook)}
            webhook={webhook}
          />
        ))}
      </ul>
      <DeliveriesDialog
        source={source}
        onClose={() => setViewing(null)}
        webhook={viewing}
      />
      <ResponsiveDialog
        alert
        onOpenChange={(open) => {
          if (!open) {
            setRemoving(null);
          }
        }}
        open={removing !== null}
      >
        <ResponsiveDialogContent>
          <ResponsiveDialogHeader>
            <ResponsiveDialogTitle>
              {m.webhooks_remove_confirm_title()}
            </ResponsiveDialogTitle>
            <ResponsiveDialogDescription>
              {m.webhooks_remove_confirm_description()}
            </ResponsiveDialogDescription>
          </ResponsiveDialogHeader>
          <ResponsiveDialogFooter>
            <Button
              onClick={() => setRemoving(null)}
              type="button"
              variant="outline"
            >
              {m.common_cancel()}
            </Button>
            <Button
              onClick={() => {
                if (removing) {
                  handleRemove(removing);
                }
                setRemoving(null);
              }}
              type="button"
              variant="destructive"
            >
              {m.webhooks_remove()}
            </Button>
          </ResponsiveDialogFooter>
        </ResponsiveDialogContent>
      </ResponsiveDialog>
    </>
  );
}

function WebhookRow({
  webhook,
  canRemove,
  isRemoving,
  onRemove,
  onViewDeliveries,
}: {
  webhook: Webhook;
  canRemove: boolean;
  isRemoving: boolean;
  onRemove: () => void;
  onViewDeliveries: () => void;
}) {
  const formatDate = useDateTimeFormatter();

  return (
    <li className="flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3">
      <span className="mt-0.5 shrink-0 text-muted-foreground">
        <IconWebhook size={18} />
      </span>
      <div className="min-w-0 flex-1 basis-48 space-y-1">
        <p className="break-all font-medium text-sm">{webhook.url}</p>
        <div className="flex flex-wrap gap-1">
          {webhook.events.map((event) => (
            <Badge key={event} variant="secondary">
              {eventLabel(event)}
            </Badge>
          ))}
          {webhook.format === "discord" ? (
            <Badge variant="outline">{m.webhooks_format_discord()}</Badge>
          ) : null}
        </div>
        <p className="text-muted-foreground text-xs">
          {m.webhooks_created_at({ date: formatDate(webhook.created_at) })}
        </p>
      </div>
      <div className="flex w-full gap-2 sm:w-auto">
        <Button
          onClick={onViewDeliveries}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconHistory size={16} />
          {m.webhooks_deliveries()}
        </Button>
        {canRemove ? (
          <Button
            disabled={isRemoving}
            onClick={onRemove}
            size="sm"
            type="button"
            variant="outline"
          >
            <IconTrash size={16} />
            {m.webhooks_remove()}
          </Button>
        ) : null}
      </div>
    </li>
  );
}
