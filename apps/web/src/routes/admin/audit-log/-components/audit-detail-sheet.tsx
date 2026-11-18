import {
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerHeader,
  DrawerTitle,
} from "@tessera/ui/components/drawer";
import { ScrollArea } from "@tessera/ui/components/scroll-area";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@tessera/ui/components/sheet";
import { useIsMobile } from "@tessera/ui/hooks/use-mobile";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { actionLabel, actorLabel, targetLabel } from "@/lib/admin/audit-events";
import type { AuditEvent } from "@/lib/admin/audit-events";
import * as m from "@/paraglide/messages";

import { OutcomeBadge } from "./audit-list";

interface AuditDetailSheetProps {
  event: AuditEvent | null;
  onClose: () => void;
}

/**
 * One entry, whole: who, what, from where, and whatever the action recorded.
 * A bottom drawer on a phone, a side sheet from `md` up, as the server log's.
 */
export function AuditDetailSheet({ event, onClose }: AuditDetailSheetProps) {
  const isMobile = useIsMobile();
  const handleOpenChange = (open: boolean) => {
    if (!open) {
      onClose();
    }
  };

  if (isMobile) {
    return (
      <Drawer
        onOpenChange={handleOpenChange}
        open={event !== null}
        showSwipeHandle
      >
        <DrawerContent>
          {event && (
            <>
              <DrawerHeader>
                <DrawerTitle className="flex items-center gap-2">
                  {actionLabel(event.action)}
                  <OutcomeBadge outcome={event.outcome} />
                </DrawerTitle>
                <DrawerDescription className="text-left font-mono text-xs">
                  {event.action}
                </DrawerDescription>
              </DrawerHeader>
              <div className="mt-4 max-h-[60svh] overflow-y-auto overscroll-contain px-4 pb-[max(1rem,env(safe-area-inset-bottom))]">
                <AuditDetailBody event={event} />
              </div>
            </>
          )}
        </DrawerContent>
      </Drawer>
    );
  }

  return (
    <Sheet onOpenChange={handleOpenChange} open={event !== null}>
      <SheetContent className="w-full sm:max-w-xl">
        {event && (
          <>
            <SheetHeader className="pr-14">
              <SheetTitle className="flex items-center gap-2">
                {actionLabel(event.action)}
                <OutcomeBadge outcome={event.outcome} />
              </SheetTitle>
              <SheetDescription className="font-mono text-xs">
                {event.action}
              </SheetDescription>
            </SheetHeader>
            <ScrollArea className="min-h-0 flex-1">
              <div className="px-6 pb-6">
                <AuditDetailBody event={event} />
              </div>
            </ScrollArea>
          </>
        )}
      </SheetContent>
    </Sheet>
  );
}

function AuditDetailBody({ event }: { event: AuditEvent }) {
  const formatDateTime = useDateTimeFormatter();
  const rows: [string, string | null | undefined][] = [
    [m.audit_col_time(), formatDateTime(event.occurred_at)],
    [m.audit_col_actor(), actorLabel(event)],
    [m.audit_detail_actor_email(), event.actor?.email],
    [m.audit_detail_actor_kind(), event.actor_kind],
    [m.audit_detail_token(), event.api_token_id],
    [m.audit_col_target(), targetLabel(event)],
    [m.audit_col_ip(), event.ip_address],
    [m.audit_detail_user_agent(), event.user_agent],
    [m.audit_detail_id(), event.id],
  ];

  return (
    <div className="flex flex-col gap-5">
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
        {rows
          .filter((row): row is [string, string] => Boolean(row[1]))
          .map(([label, value]) => (
            <div className="contents" key={label}>
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="min-w-0 break-all font-mono text-xs leading-5">
                {value}
              </dd>
            </div>
          ))}
      </dl>
      <section className="flex flex-col gap-1.5">
        <h3 className="font-medium text-muted-foreground text-xs">
          {m.audit_detail_metadata()}
        </h3>
        {event.metadata ? (
          <pre className="overflow-x-auto rounded-md border bg-muted/40 p-2.5 font-mono text-xs leading-relaxed">
            {JSON.stringify(event.metadata, null, 2)}
          </pre>
        ) : (
          <p className="text-muted-foreground text-sm">
            {m.audit_detail_no_metadata()}
          </p>
        )}
      </section>
    </div>
  );
}
