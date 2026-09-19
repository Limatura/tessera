import { IconChevronDown, IconDevices } from "@tabler/icons-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@tessera/ui/components/collapsible";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { toast } from "@tessera/ui/components/toast";

import type { Session } from "@/hooks/use-sessions";
import { useRevokeSession, useSessions } from "@/hooks/use-sessions";
import { createDateTimeFormatter } from "@/lib/format";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { useAppearanceStore } from "@/stores/appearance";

import { EmptyBox, Section } from "./section";
import { SessionRow } from "./session-row";

/**
 * 「登出其他裝置」不放在這裡的標題列 —— 它就在下方的危險區域裡,
 * 同一頁擺兩顆一樣的按鈕只會讓人猶豫哪顆才是真的。
 */
export function SessionsSection() {
  const sessions = useSessions();
  const revokeOne = useRevokeSession();
  const timeFormat = useAppearanceStore((state) => state.timeFormat);
  // locale 一變 root 會 remount,所以直接讀就好(跟 shares-table 同一個理由)。
  const formatDate = createDateTimeFormatter(getLocale(), timeFormat);

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

/** 新的在上面。同一組裡目前這台永遠排第一,那是讀這頁的人的定位點。 */
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
   * 還活著的跟已經結束的分兩區。
   *
   * 已結束的不能刪掉(issue #90:被撤銷的那筆常常就是「有沒有別人進來過」的答案),
   * 但它們只會愈積愈多,而且沒有任何動作可做。折起來而不是塞進一個會自己捲的框:
   * 巢狀捲動區在設定頁裡會搶走滾輪、在手機上更難操作,而且捲軸不會告訴你
   * 「下面還有 23 筆」—— 折疊的標題會。
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

/** 預設收合 —— 平常沒人要看,但要查的時候不能查不到。 */
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
              // 已結束的沒有撤銷鍵,revoking 狀態對它們沒有意義。
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
