import { IconChevronDown, IconDevices } from "@tabler/icons-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@tessera/ui/components/collapsible";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { Session } from "@/hooks/use-sessions";
import { useRevokeSession, useSessions } from "@/hooks/use-sessions";
import * as m from "@/paraglide/messages";

import { EmptyBox, Section } from "./section";
import { SessionRow } from "./session-row";

/**
 * "Sign out other devices" is not in this title row: it is in the danger zone below,
 * and two identical buttons on one page leave people unsure which one is real.
 */
export function SessionsSection() {
  const sessions = useSessions();
  const revokeOne = useRevokeSession();
  const formatDate = useDateTimeFormatter();

  const handleRevoke = (id: string) => {
    revokeOne.mutate(id, {
      onError: () =>
        toast.add({
          description: m.common_try_again(),
          title: m.user_sessions_revoke_failed(),
          type: "error",
        }),
      onSuccess: () =>
        toast.add({ title: m.user_sessions_revoke_done(), type: "success" }),
    });
  };

  return (
    <Section
      description={m.user_sessions_description()}
      title={m.user_sessions_title()}
    >
      <SessionsList
        formatDate={formatDate}
        isError={sessions.isError}
        isPending={sessions.isPending}
        onRevoke={handleRevoke}
        revokingId={revokeOne.isPending ? revokeOne.variables : null}
        sessions={sessions.data}
      />
    </Section>
  );
}

/** Newest first. Within a group the current device always comes first: it anchors the reader. */
function byRecency(a: Session, b: Session) {
  if (a.current !== b.current) {
    return a.current ? -1 : 1;
  }
  return Date.parse(b.created_at) - Date.parse(a.created_at);
}

function SessionsList({
  isPending,
  isError,
  sessions,
  formatDate,
  onRevoke,
  revokingId,
}: {
  isPending: boolean;
  isError: boolean;
  sessions: Session[] | undefined;
  formatDate: (value: string | null | undefined) => string;
  onRevoke: (id: string) => void;
  revokingId: string | null;
}) {
  if (isPending) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-16 w-full" />
      </div>
    );
  }

  if (isError) {
    return (
      <p className="text-destructive text-sm">{m.user_sessions_error()}</p>
    );
  }

  if (!sessions || sessions.length === 0) {
    return (
      <EmptyBox icon={<IconDevices size={20} />}>
        {m.user_sessions_empty()}
      </EmptyBox>
    );
  }

  /**
   * Live and ended sessions are split.
   *
   * Ended sessions cannot be deleted (a revoked one is often the answer to "did someone
   * else get in"), but they keep piling up with nothing to do on them. They fold away
   * instead of sitting in a scrolling box: nested scroll areas steal the wheel on a
   * settings page and are worse on phones, and a scrollbar does not say "23 more
   * below". A collapsed heading does.
   */
  const active = sessions
    .filter((session) => session.active)
    .toSorted(byRecency);
  const ended = sessions
    .filter((session) => !session.active)
    .toSorted(byRecency);

  return (
    <div className="space-y-4">
      {active.length > 0 ? (
        <ul className="space-y-3">
          {active.map((session) => (
            <SessionRow
              formatDate={formatDate}
              isRevoking={revokingId === session.id}
              key={session.id}
              onRevoke={onRevoke}
              session={session}
            />
          ))}
        </ul>
      ) : null}

      {ended.length > 0 ? (
        <EndedSessions
          formatDate={formatDate}
          onRevoke={onRevoke}
          sessions={ended}
        />
      ) : null}
    </div>
  );
}

/** Collapsed by default: rarely looked at, but it must be there when needed. */
function EndedSessions({
  sessions,
  formatDate,
  onRevoke,
}: {
  sessions: Session[];
  formatDate: (value: string | null | undefined) => string;
  onRevoke: (id: string) => void;
}) {
  return (
    <Collapsible>
      <CollapsibleTrigger className="group flex w-full items-center gap-2 rounded-lg border border-dashed px-4 py-3 text-muted-foreground text-sm hover:bg-muted/50">
        <IconChevronDown
          className="transition-transform group-data-panel-open:rotate-180"
          size={16}
        />
        {m.user_sessions_ended_group({ count: sessions.length })}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="space-y-3 pt-3">
          {sessions.map((session) => (
            <SessionRow
              formatDate={formatDate}
              // Ended sessions have no revoke button, so the revoking state means nothing for them.
              isRevoking={false}
              key={session.id}
              onRevoke={onRevoke}
              session={session}
            />
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  );
}
