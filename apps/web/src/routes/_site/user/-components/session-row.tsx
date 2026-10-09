import {
  IconDeviceDesktop,
  IconDeviceMobile,
  IconTerminal2,
} from "@tabler/icons-react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@tessera/ui/components/alert-dialog";
import { Badge } from "@tessera/ui/components/badge";
import { buttonVariants } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";
import type * as React from "react";

import type { Session } from "@/hooks/use-sessions";
import { formatUserAgent, parseUserAgent } from "@/lib/user-agent";
import * as m from "@/paraglide/messages";

/** `ended_reason` 是後端固定字串,沒對到的值(後端加了新的、我們還沒跟上)就原樣顯示,總比吞掉答案好。 */
function endedReasonLabel(reason: string): string {
  switch (reason) {
    case "signed_out": {
      return m.user_sessions_ended_signed_out();
    }
    case "revoked": {
      return m.user_sessions_ended_revoked();
    }
    case "password_changed": {
      return m.user_sessions_ended_password_changed();
    }
    case "password_reset": {
      return m.user_sessions_ended_password_reset();
    }
    case "account_disabled": {
      return m.user_sessions_ended_account_disabled();
    }
    default: {
      return reason;
    }
  }
}

function DeviceIcon({ userAgent }: { userAgent: string | null | undefined }) {
  const parsed = parseUserAgent(userAgent);

  if (parsed?.browser === "curl" || parsed?.browser === "Tessera CLI") {
    return <IconTerminal2 size={18} />;
  }
  if (parsed?.os === "iOS" || parsed?.os === "Android") {
    return <IconDeviceMobile size={18} />;
  }
  return <IconDeviceDesktop size={18} />;
}

/**
 * meta 清單裡的一列。手機上標籤靠左、值靠右疊成小表,窄螢幕好掃;桌機
 * (sm+)外層 `dl` 變 flex-wrap,縮成「標籤 值」小團並排。
 *
 * 不用間隔點分隔——點是獨立節點,換行時會孤兒化掉到下一行開頭,改用
 * `gap-x-4` 拉開就不會。
 */
function MetaItem({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-baseline justify-between gap-2 sm:justify-start sm:gap-1.5">
      <dt className="shrink-0">{label}</dt>
      <dd className="min-w-0 truncate text-foreground/80">{children}</dd>
    </div>
  );
}

export function SessionRow({
  session,
  formatDate,
  onRevoke,
  isRevoking,
}: {
  session: Session;
  formatDate: (value: string | null | undefined) => string;
  onRevoke: (id: string) => void;
  isRevoking: boolean;
}) {
  const device = formatUserAgent(session.created_user_agent);

  return (
    /*
     * `flex-wrap` + 撤銷鍵的 `w-full sm:w-auto`:手機上換行變滿版(拇指好按),
     * 桌機同一列。用 wrap 而非把按鈕寫兩次,AlertDialog 只有一份,不會有兩個
     * dialog 同時掛在 DOM 上。
     */
    <li
      className={cn(
        "flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3",
        !session.active && "bg-muted/40"
      )}
    >
      <span className="mt-0.5 shrink-0 text-muted-foreground">
        <DeviceIcon userAgent={session.created_user_agent} />
      </span>

      {/* `basis-48` 是換行門檻:剩下的寬度不夠放 12rem 時按鈕就自己掉下去。 */}
      <div className="min-w-0 flex-1 basis-48 space-y-1.5">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          {/*
            解析失敗就拿原始字串當標題,再失敗(UA 是 null)才用佔位字;
            `break-words` 讓長字串在手機上斷行而不是撐爆卡片。
            解析成功時原始字串只收進 `title`——觸控裝置 hover 不到是唯一取捨,
            但辨識「這不是我」實際靠下面的 IP 跟時間,原始 UA 只是最後一道,
            獨佔一行只會讓卡片在手機上更高。
          */}
          <span
            className="break-words font-medium text-sm"
            title={session.created_user_agent ?? undefined}
          >
            {device ??
              session.created_user_agent ??
              m.user_sessions_unknown_device()}
          </span>
          {session.current ? (
            <Badge variant="secondary">{m.user_sessions_current()}</Badge>
          ) : null}
          {session.active ? null : (
            <Badge variant="outline">
              {session.ended_reason
                ? endedReasonLabel(session.ended_reason)
                : m.user_sessions_ended_expired()}
            </Badge>
          )}
        </div>

        {/*
          沒有「到期」這一項。issue #90:存活與否要看 `active`,不要讓人拿
          `expires_at` 自己跟時鐘比——擺一個到期時間等於邀請人那麼做,還容易
          跟「最後活動」看混。「還能不能用」已經由撤銷鍵跟結束原因 badge 表達了。
        */}
        <dl className="space-y-0.5 text-muted-foreground text-xs sm:flex sm:flex-wrap sm:gap-x-4 sm:space-y-0">
          <MetaItem label={m.user_sessions_ip()}>
            {/* 後端在沒設 trusted_proxies 時寧可不記,也不記一個假位址。 */}
            <span className="font-mono">
              {session.created_ip ?? m.user_sessions_ip_unknown()}
            </span>
          </MetaItem>
          <MetaItem label={m.user_sessions_created()}>
            {formatDate(session.created_at)}
          </MetaItem>
          <MetaItem label={m.user_sessions_last_seen()}>
            {formatDate(session.last_seen_at)}
          </MetaItem>
        </dl>
      </div>

      {/* 撤銷鍵只給還活著的。`active` 直接用後端給的,不拿 expires_at 跟本機時鐘比。 */}
      {session.active && !session.current ? (
        <AlertDialog>
          <AlertDialogTrigger
            className={cn(
              buttonVariants({ size: "sm", variant: "outline" }),
              "w-full sm:w-auto"
            )}
            disabled={isRevoking}
          >
            {m.user_sessions_revoke()}
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {m.user_sessions_revoke_confirm_title()}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {m.user_sessions_revoke_confirm_description()}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{m.common_cancel()}</AlertDialogCancel>
              <AlertDialogAction onClick={() => onRevoke(session.id)}>
                {m.user_sessions_revoke()}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      ) : null}
    </li>
  );
}
