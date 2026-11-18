import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";

import {
  ListEmpty,
  ListError,
  ListSkeleton,
} from "@/components/admin/list-state";
import type { useAuditEvents } from "@/hooks/admin/use-audit-events";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import {
  actionLabel,
  actorLabel,
  OUTCOME_LABELS,
  targetLabel,
} from "@/lib/admin/audit-events";
import type { AuditEvent, AuditOutcome } from "@/lib/admin/audit-events";
import * as m from "@/paraglide/messages";

type Events = ReturnType<typeof useAuditEvents>;

interface AuditListProps {
  events: Events;
  onSelect: (event: AuditEvent) => void;
}

export function AuditList({ events, onSelect }: AuditListProps) {
  if (events.isPending) {
    return <ListSkeleton rows={8} />;
  }
  if (events.isError) {
    return (
      <ListError
        isRetrying={events.isFetching}
        message={m.audit_load_error()}
        onRetry={() => events.refetch()}
      />
    );
  }

  const items = events.data.pages.flatMap((page) => page.items);
  if (items.length === 0) {
    return <ListEmpty>{m.audit_empty()}</ListEmpty>;
  }

  return (
    <div className="flex flex-col gap-3">
      <ul className="flex flex-col gap-2 md:hidden">
        {items.map((event) => (
          <AuditCard event={event} key={event.id} onSelect={onSelect} />
        ))}
      </ul>
      <div className="hidden md:block">
        <AuditTable items={items} onSelect={onSelect} />
      </div>
      <LoadMore events={events} />
    </div>
  );
}

function LoadMore({ events }: { events: Events }) {
  if (!events.hasNextPage) {
    return null;
  }
  return (
    <Button
      className="self-center"
      disabled={events.isFetchingNextPage}
      onClick={() => events.fetchNextPage()}
      type="button"
      variant="outline"
    >
      {events.isFetchingNextPage ? m.common_loading() : m.audit_load_more()}
    </Button>
  );
}

export function OutcomeBadge({ outcome }: { outcome: string }) {
  const known = outcome === "success" || outcome === "failure";
  return (
    <Badge variant={outcome === "failure" ? "destructive" : "secondary"}>
      {known ? OUTCOME_LABELS[outcome as AuditOutcome]() : outcome}
    </Badge>
  );
}

function AuditTable({
  items,
  onSelect,
}: {
  items: AuditEvent[];
  onSelect: (event: AuditEvent) => void;
}) {
  const formatDateTime = useDateTimeFormatter();

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{m.audit_col_time()}</TableHead>
          <TableHead>{m.audit_col_action()}</TableHead>
          <TableHead>{m.audit_col_outcome()}</TableHead>
          <TableHead>{m.audit_col_actor()}</TableHead>
          <TableHead>{m.audit_col_target()}</TableHead>
          <TableHead>{m.audit_col_ip()}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {items.map((event) => (
          <TableRow
            className="cursor-pointer"
            key={event.id}
            onClick={() => onSelect(event)}
          >
            <TableCell className="whitespace-nowrap text-muted-foreground tabular-nums">
              {formatDateTime(event.occurred_at)}
            </TableCell>
            <TableCell>
              {/* A real button inside the row, so the keyboard reaches it too. */}
              <button
                className="text-left font-medium hover:underline focus-visible:underline focus-visible:outline-none"
                onClick={(click) => {
                  click.stopPropagation();
                  onSelect(event);
                }}
                type="button"
              >
                {actionLabel(event.action)}
              </button>
            </TableCell>
            <TableCell>
              <OutcomeBadge outcome={event.outcome} />
            </TableCell>
            <TableCell className="max-w-48 truncate">
              {actorLabel(event)}
            </TableCell>
            <TableCell className="max-w-64 truncate font-mono text-xs">
              {targetLabel(event) ?? "—"}
            </TableCell>
            <TableCell className="font-mono text-muted-foreground text-xs">
              {event.ip_address ?? "—"}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function AuditCard({
  event,
  onSelect,
}: {
  event: AuditEvent;
  onSelect: (event: AuditEvent) => void;
}) {
  const formatDateTime = useDateTimeFormatter();
  const target = targetLabel(event);

  return (
    <li>
      <button
        className="flex w-full flex-col gap-1.5 rounded-lg border p-3 text-left"
        onClick={() => onSelect(event)}
        type="button"
      >
        <span className="flex items-center justify-between gap-3">
          <span className="truncate font-medium">
            {actionLabel(event.action)}
          </span>
          <OutcomeBadge outcome={event.outcome} />
        </span>
        <span className="truncate text-muted-foreground text-xs">
          {actorLabel(event)} · {formatDateTime(event.occurred_at)}
        </span>
        {target && (
          <span className="truncate font-mono text-muted-foreground text-xs">
            {target}
          </span>
        )}
      </button>
    </li>
  );
}
