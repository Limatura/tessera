import { Link } from "@tanstack/react-router";
import { cn } from "@tessera/ui/lib/utils";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { expiryLabel, isExpired } from "@/lib/expiry-label";
import * as m from "@/paraglide/messages";

import { ShareBadges } from "./share-fields";
import type { Share } from "./share-fields";
import { ShareRowActions } from "./share-row-actions";

/**
 * 窄螢幕的版面:一筆一張卡片。
 *
 * 之前一列塞了取件碼、標籤、三組日期、箭頭跟兩顆按鈕,日期被擠到每個都折成兩行。
 * 現在只有一件事放大 —— 還能不能取、什麼時候到期(手機上回來看清單,多半是想知道這個),
 * 建立、最後取件退到次要的一行。按鈕搬到右上角,不再佔掉內容的寬度。
 *
 * 整張卡片都是連結(用 `after:` 撐滿),按鈕用 `relative` 浮在上面,
 * 因為 `<button>` 不能放進 `<a>` 裡。
 */
export function SharesList({
  shares,
  onManage,
  onDelete,
}: {
  shares: Share[];
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  return (
    <ul className="flex flex-col gap-3">
      {shares.map((share) => (
        <ShareCard
          key={share.id}
          onDelete={onDelete}
          onManage={onManage}
          share={share}
        />
      ))}
    </ul>
  );
}

function ShareCard({
  share,
  onManage,
  onDelete,
}: {
  share: Share;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  const formatDate = useDateTimeFormatter();
  const expired = isExpired(share.expires_at);

  return (
    <li className="relative rounded-xl border bg-card transition-colors focus-within:ring-2 focus-within:ring-ring active:bg-muted/40">
      <div className="flex items-start justify-between gap-2 p-4 pb-3">
        <div className="flex min-w-0 flex-col gap-2">
          <Link
            className="truncate font-medium font-mono text-lg outline-none after:absolute after:inset-0 after:rounded-xl"
            params={{ code: share.code }}
            to="/s/$code"
          >
            {share.code}
          </Link>
          <ShareBadges share={share} />
        </div>
        <div className="relative z-10 -mt-2 -mr-2 shrink-0">
          <ShareRowActions
            onDelete={onDelete}
            onManage={onManage}
            share={share}
          />
        </div>
      </div>

      <div className="space-y-1 border-t px-4 py-3">
        <p className={cn("font-medium text-sm", expired && "text-destructive")}>
          {expiryLabel(share.expires_at)}
          {share.expires_at && (
            <span className="ml-2 font-normal text-muted-foreground text-xs tabular-nums">
              {formatDate(share.expires_at)}
            </span>
          )}
        </p>
        <dl className="flex flex-wrap gap-x-4 gap-y-0.5 text-muted-foreground text-xs tabular-nums">
          <Meta label={m.shares_col_created()}>
            {formatDate(share.created_at)}
          </Meta>
          <Meta label={m.shares_col_accessed()}>
            {formatDate(share.last_accessed_at)}
          </Meta>
        </dl>
      </div>
    </li>
  );
}

function Meta({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex gap-1.5">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}
