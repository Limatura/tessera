import {
  IconChevronLeft,
  IconChevronRight,
  IconInbox,
} from "@tabler/icons-react";
import { useQueryClient } from "@tanstack/react-query";
import { Link, Navigate } from "@tanstack/react-router";
import { buttonVariants } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";
import { useState } from "react";

import { SHARES_PAGE_SIZE, useMyShares } from "@/hooks/use-shares";
import { toResult } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";

import { ManageShareDialog } from "../../-components/manage-share-dialog";
import type { ManagedShare } from "../../-components/manage-share-dialog";
import { DeleteShareDialog } from "./delete-share-dialog";
import type { Share } from "./share-fields";
import { SharesList } from "./shares-list";
import { SharesTable } from "./shares-table";

/**
 * 「我的分享」的資料與分頁。版面依寬度二選一:窄螢幕列表、寬螢幕表格。
 *
 * 用 CSS 切而不是 `useIsMobile`:兩份都渲染、只顯示一份,第一次畫面就是對的版面,
 * 不會先閃一下表格再換成列表。一頁最多 20 筆,多出的那份 DOM 不值一提。
 */
export function MyShares({ page }: { page: number }) {
  const offset = (page - 1) * SHARES_PAGE_SIZE;
  const shares = useMyShares(offset);
  const [deleting, setDeleting] = useState<Share | null>(null);
  const [managing, setManaging] = useState<ManagedShare | null>(null);
  const [manageOpen, setManageOpen] = useState(false);
  const queryClient = useQueryClient();

  // 期限、次數一改,列表上的「到期」那欄就舊了;刪掉的話總數跟分頁也變了。
  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: ["shares", "mine"] });

  const handleManage = (share: Share) => {
    setManaging(toManaged(share));
    setManageOpen(true);
  };

  if (shares.isPending) {
    return (
      <p className="text-muted-foreground text-sm">{m.shares_loading()}</p>
    );
  }

  if (shares.isError) {
    return <p className="text-destructive text-sm">{m.shares_error()}</p>;
  }

  const rows = shares.data.data;
  const { total } = shares.data.meta;

  // 刪掉最後一頁的最後一筆,這一頁就空了 —— 退回現在的最後一頁,而不是顯示「還沒有任何分享」。
  if (rows.length === 0 && page > 1) {
    return (
      <Navigate
        replace
        search={{ page: Math.max(1, Math.ceil(total / SHARES_PAGE_SIZE)) }}
        to="/shares"
      />
    );
  }

  if (rows.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-6 py-10 text-muted-foreground text-sm">
        <IconInbox size={20} />
        {m.shares_empty()}
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="md:hidden">
        <SharesList
          onDelete={setDeleting}
          onManage={handleManage}
          shares={rows}
        />
      </div>
      <div className="hidden md:block">
        <SharesTable
          onDelete={setDeleting}
          onManage={handleManage}
          shares={rows}
        />
      </div>

      <SharesPagination offset={offset} page={page} total={total} />

      <DeleteShareDialog
        onOpenChange={(open) => {
          if (!open) {
            setDeleting(null);
          }
        }}
        share={deleting}
      />

      <ManageShareDialog
        onDeleted={refresh}
        onOpenChange={setManageOpen}
        onSettingsChange={refresh}
        open={manageOpen}
        share={managing}
      />
    </div>
  );
}

/** 列表裡的資料就是 server 剛給的,不用再問一次。登入者靠 session 授權,不帶 token。 */
function toManaged(share: Share): ManagedShare {
  const result = toResult(share);
  return {
    code: share.code,
    initial: {
      burnAfterReading: share.uses_remaining === 1,
      expiresInMinutes: result.expiresInMinutes,
      hasPassword: share.has_password,
    },
    shortUrl: result.shortUrl,
    title: result.fileName,
  };
}

/**
 * 上一頁／下一頁用 `Link` 而不是 button,滑鼠中鍵開新分頁、複製連結都能用,
 * 到頭時渲染成停用的 span —— 停用的連結沒有「按不動」這種狀態,只能不給它 href。
 */
function SharesPagination({
  page,
  offset,
  total,
}: {
  page: number;
  offset: number;
  total: number;
}) {
  const lastPage = Math.max(1, Math.ceil(total / SHARES_PAGE_SIZE));

  if (lastPage === 1) {
    return null;
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
      <p className="text-muted-foreground text-sm tabular-nums">
        {m.shares_range({
          from: offset + 1,
          to: Math.min(offset + SHARES_PAGE_SIZE, total),
          total,
        })}
      </p>
      <div className="flex items-center gap-2">
        <PageLink disabled={page <= 1} label={m.shares_prev()} to={page - 1}>
          <IconChevronLeft size={16} />
          {m.shares_prev()}
        </PageLink>
        <PageLink
          disabled={page >= lastPage}
          label={m.shares_next()}
          to={page + 1}
        >
          {m.shares_next()}
          <IconChevronRight size={16} />
        </PageLink>
      </div>
    </div>
  );
}

function PageLink({
  to,
  disabled,
  label,
  children,
}: {
  to: number;
  disabled: boolean;
  label: string;
  children: React.ReactNode;
}) {
  const className = cn(
    buttonVariants({ size: "sm", variant: "outline" }),
    disabled && "pointer-events-none opacity-50"
  );

  if (disabled) {
    return (
      <span aria-disabled className={className}>
        {children}
      </span>
    );
  }

  return (
    <Link
      aria-label={label}
      className={className}
      search={{ page: to }}
      to="/shares"
    >
      {children}
    </Link>
  );
}
