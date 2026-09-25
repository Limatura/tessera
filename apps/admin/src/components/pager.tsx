import { IconChevronLeft, IconChevronRight } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { buttonVariants } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";
import { z } from "zod";

/**
 * 頁碼放 search 而不是路徑,重新整理跟複製網址都會回到同一頁。
 * 用 1-based 的 `page` 而不是 `offset`,網址對人來說才讀得懂。
 */
export const pageSearchSchema = z.object({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  page: z.coerce.number().int().min(1).catch(1),
});

type PagedRoute = "/all-shares" | "/shares" | "/users";

/**
 * 上一頁／下一頁用 `Link` 而不是 button,中鍵開新分頁、複製連結都能用,
 * 到頭時渲染成停用的 span —— 停用的連結沒有「按不動」這種狀態,只能不給它 href。
 *
 * 只有一頁時不顯示。`total` 是整份的總數,不是這一頁的長度。
 */
export function Pager({
  to,
  page,
  pageSize,
  total,
}: {
  to: PagedRoute;
  page: number;
  pageSize: number;
  total: number;
}) {
  const lastPage = Math.max(1, Math.ceil(total / pageSize));

  if (lastPage === 1) {
    return null;
  }

  const offset = (page - 1) * pageSize;

  return (
    <div className="mt-4 flex items-center justify-between gap-4">
      <p className="text-sm text-muted-foreground tabular-nums">
        第 {Math.min(offset + 1, total)}–{Math.min(offset + pageSize, total)}{" "}
        筆,共 {total} 筆
      </p>
      <div className="flex items-center gap-2">
        <PageLink disabled={page <= 1} label="上一頁" page={page - 1} to={to}>
          <IconChevronLeft size={16} />
          上一頁
        </PageLink>
        <PageLink
          disabled={page >= lastPage}
          label="下一頁"
          page={page + 1}
          to={to}
        >
          下一頁
          <IconChevronRight size={16} />
        </PageLink>
      </div>
    </div>
  );
}

function PageLink({
  to,
  page,
  disabled,
  label,
  children,
}: {
  to: PagedRoute;
  page: number;
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
    // 保留其他 search 參數(例如使用者頁的 `deleted`),只換頁碼。
    <Link
      aria-label={label}
      className={className}
      search={(previous) => ({ ...previous, page })}
      to={to}
    >
      {children}
    </Link>
  );
}
