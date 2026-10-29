import { IconRefresh } from "@tabler/icons-react";
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

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import * as m from "@/paraglide/messages";

import { deliveryStateLabel, eventLabel } from "./labels";
import type { Delivery, Webhook, WebhooksSource } from "./types";
import { useWebhookDeliveries } from "./use-webhooks";

/** `webhook` is `null` while closed, so the query only runs for the one being looked at. */
export function DeliveriesDialog({
  source,
  webhook,
  onClose,
}: {
  source: WebhooksSource;
  webhook: Webhook | null;
  onClose: () => void;
}) {
  const deliveries = useWebhookDeliveries(
    source,
    webhook?.id ?? "",
    webhook !== null
  );

  const renderBody = () => {
    if (deliveries.isPending) {
      return <Skeleton className="h-24 w-full" />;
    }
    if (deliveries.isError) {
      return (
        <p className="text-destructive text-sm" role="alert">
          {m.webhooks_deliveries_error()}
        </p>
      );
    }
    if (deliveries.data.length === 0) {
      return (
        <p className="py-6 text-center text-muted-foreground text-sm">
          {m.webhooks_deliveries_empty()}
        </p>
      );
    }
    return (
      <ul className="max-h-96 space-y-2 overflow-y-auto">
        {deliveries.data.map((delivery) => (
          <DeliveryRow delivery={delivery} key={delivery.id} />
        ))}
      </ul>
    );
  };

  return (
    <ResponsiveDialog
      onOpenChange={(open) => {
        if (!open) {
          onClose();
        }
      }}
      open={webhook !== null}
    >
      <ResponsiveDialogContent>
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>
            {m.webhooks_deliveries_title()}
          </ResponsiveDialogTitle>
          <ResponsiveDialogDescription className="break-all">
            {webhook?.url}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        {renderBody()}
        <ResponsiveDialogFooter>
          <Button
            disabled={deliveries.isFetching}
            onClick={() => deliveries.refetch()}
            type="button"
            variant="outline"
          >
            <IconRefresh size={16} />
            {m.webhooks_refresh()}
          </Button>
          <Button onClick={onClose} type="button">
            {m.webhooks_done()}
          </Button>
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

function stateVariant(state: string) {
  if (state === "failed") {
    return "destructive" as const;
  }
  return state === "delivered" ? ("secondary" as const) : ("outline" as const);
}

function DeliveryRow({ delivery }: { delivery: Delivery }) {
  const formatDate = useDateTimeFormatter();
  const detail = [
    m.webhooks_attempts({ count: delivery.attempts }),
    delivery.last_status ? `HTTP ${delivery.last_status}` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <li className="space-y-1 rounded-lg border p-3 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="font-medium">{eventLabel(delivery.event)}</span>
        <Badge variant={stateVariant(delivery.state)}>
          {deliveryStateLabel(delivery.state)}
        </Badge>
      </div>
      <p className="text-muted-foreground text-xs">
        {formatDate(delivery.created_at)} · {detail}
      </p>
      {delivery.state === "pending" && delivery.next_attempt_at ? (
        <p className="text-muted-foreground text-xs">
          {m.webhooks_next_attempt({
            date: formatDate(delivery.next_attempt_at),
          })}
        </p>
      ) : null}
      {delivery.last_error ? (
        <p className="break-all text-destructive text-xs">
          {delivery.last_error}
        </p>
      ) : null}
    </li>
  );
}
