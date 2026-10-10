import { Link } from "@tanstack/react-router";
import { Checkbox } from "@tessera/ui/components/checkbox";
import { cn } from "@tessera/ui/lib/utils";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { RowSelection } from "@/hooks/use-row-selection";
import { expiryLabel } from "@/lib/expiry-label";
import * as m from "@/paraglide/messages";

import { ShareBadges, ShareStateBadge, shareState } from "./share-fields";
import type { Share } from "./share-fields";
import { ShareRowActions } from "./share-row-actions";

/**
 * 窄螢幕的版面:一筆一張卡片。
 *
 * 之前一列塞了取件碼、標籤、三組日期、箭頭跟兩顆按鈕,日期被擠到每個都折成兩行。
 * 現在只有一件事放大 —— 還能不能取、什麼時候到期(手機上回來看清單,多半是想知道這個),
 * 建立、最後取件退到次要的一行。按鈕搬到右上角,不再佔掉內容的寬度。
 *
 * 整張卡片都是連結(用 `after:` 撐滿),勾選框跟按鈕用 `relative` 浮在上面,
 * 因為 `<button>` 不能放進 `<a>` 裡。勾選框放取件碼左邊、對齊第一行,
 * 拇指點得到又不會跟右上角的刪除按鈕靠在一起誤觸。
 *
 * 已失效的卡片退到背景(淡底、虛線框、取件碼退色),到期那行換成失效原因。
 * 不用紅色:過期是正常的生命週期,不是錯誤,紅色留給真的要處理的事。
 */
export function SharesList({
  shares,
  selection,
  onManage,
  onDelete,
}: {
  shares: Share[];
  selection: RowSelection;
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
          onSelectedChange={(checked) => selection.setOne(share.code, checked)}
          selected={selection.isSelected(share.code)}
          share={share}
        />
      ))}
    </ul>
  );
}

function ShareCard({
  share,
  selected,
  onSelectedChange,
  onManage,
  onDelete,
}: {
  share: Share;
  selected: boolean;
  onSelectedChange: (checked: boolean) => void;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  const formatDate = useDateTimeFormatter();
  const state = shareState(share);
  const dead = state !== "live";

  return (
    <li
      className={cn(
        "relative rounded-xl border bg-card transition-colors focus-within:ring-2 focus-within:ring-ring active:bg-muted/40",
        dead && "border-dashed bg-muted/30",
        selected && "border-solid border-primary bg-muted/50"
      )}
    >
      <div className="flex items-start gap-3 p-4 pb-3">
        <Checkbox
          aria-label={m.selection_select_row({ label: share.code })}
          checked={selected}
          className="relative z-10 mt-1.5"
          onCheckedChange={onSelectedChange}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <Link
            className={cn(
              "truncate font-medium font-mono text-lg outline-none after:absolute after:inset-0 after:rounded-xl",
              dead && "text-muted-foreground"
            )}
            params={{ code: share.code }}
            to="/s/$code"
          >
            {share.code}
          </Link>
          <span className={cn(dead && "opacity-60")}>
            <ShareBadges share={share} />
          </span>
        </div>
        <div className="relative z-10 -mt-2 -mr-2 shrink-0">
          <ShareRowActions
            onDelete={onDelete}
            onManage={onManage}
            share={share}
          />
        </div>
      </div>

      <div className="space-y-1 border-t py-3 pr-4 pl-11">
        {dead ? (
          <p className="flex items-center gap-2 text-sm">
            <ShareStateBadge state={state} />
            {state === "expired" && share.expires_at && (
              <span className="text-muted-foreground text-xs tabular-nums">
                {formatDate(share.expires_at)}
              </span>
            )}
          </p>
        ) : (
          <p className="font-medium text-sm">
            {expiryLabel(share.expires_at)}
            {share.expires_at && (
              <span className="ml-2 font-normal text-muted-foreground text-xs tabular-nums">
                {formatDate(share.expires_at)}
              </span>
            )}
          </p>
        )}
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
