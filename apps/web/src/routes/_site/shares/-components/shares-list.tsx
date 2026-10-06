import { IconChevronRight } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { ShareBadges, createDateFormatter } from "./share-fields";
import type { Share } from "./share-fields";

/**
 * 窄螢幕的版面:一筆一列,整列都是連結。
 *
 * 表格那邊只有取件碼能點,在手機上那是一個很小的目標;這裡整列都能點,
 * 右邊的箭頭說明它會開到哪裡去。時間欄改成標籤＋值的兩欄,到期放第一個 ——
 * 在手機上回來看清單,多半是想知道哪些還活著。
 */
export function SharesList({ shares }: { shares: Share[] }) {
  const formatDate = createDateFormatter(getLocale());

  return (
    <ul className="divide-y overflow-hidden rounded-lg border">
      {shares.map((share) => (
        <li key={share.id}>
          <Link
            className="flex items-center gap-3 p-4 outline-none transition-colors hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset active:bg-muted/60"
            params={{ code: share.code }}
            to="/s/$code"
          >
            <div className="min-w-0 flex-1 space-y-3">
              <div className="flex items-center justify-between gap-3">
                <span className="min-w-0 truncate font-medium font-mono text-base">
                  {share.code}
                </span>
                <ShareBadges share={share} />
              </div>
              <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
                <ShareField label={m.shares_col_expires()}>
                  {share.expires_at
                    ? formatDate(share.expires_at)
                    : m.shares_never_expires()}
                </ShareField>
                <ShareField label={m.shares_col_accessed()}>
                  {formatDate(share.last_accessed_at)}
                </ShareField>
                <ShareField label={m.shares_col_created()}>
                  {formatDate(share.created_at)}
                </ShareField>
              </dl>
            </div>
            <IconChevronRight
              aria-hidden
              className="shrink-0 text-muted-foreground"
              size={16}
            />
          </Link>
        </li>
      ))}
    </ul>
  );
}

function ShareField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="text-right tabular-nums">{children}</dd>
    </>
  );
}
