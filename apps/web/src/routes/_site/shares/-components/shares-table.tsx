import {
  IconChevronLeft,
  IconChevronRight,
  IconInbox,
} from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { Badge } from "@tessera/ui/components/badge";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { cn } from "@tessera/ui/lib/utils";

import { SHARES_PAGE_SIZE, useMyShares } from "@/hooks/use-shares";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

export function SharesTable({ page }: { page: number }) {
  const offset = (page - 1) * SHARES_PAGE_SIZE;
  const shares = useMyShares(offset);
  // 跟著介面語言走,不是寫死 zh-TW。locale 一變 root 會 remount,所以這裡直接讀就好。
  const formatDate = createDateFormatter(getLocale());

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
      {/* 表格在窄螢幕會超出寬度,讓它自己捲而不是把整頁撐開。 */}
      <div className="overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{m.shares_col_code()}</TableHead>
              <TableHead>{m.shares_col_kind()}</TableHead>
              <TableHead>{m.shares_col_created()}</TableHead>
              <TableHead>{m.shares_col_accessed()}</TableHead>
              <TableHead>{m.shares_col_expires()}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((share) => (
              <TableRow key={share.id}>
                <TableCell className="font-medium font-mono">
                  <Link
                    className="hover:underline"
                    params={{ code: share.code }}
                    to="/s/$code"
                  >
                    {share.code}
                  </Link>
                </TableCell>
                <TableCell>
                  <Badge variant="secondary">
                    {shareKindLabel(share.kind)}
                  </Badge>
                  {share.has_password && (
                    <Badge className="ml-1" variant="outline">
                      {m.shares_needs_password()}
                    </Badge>
                  )}
                </TableCell>
                <TableCell className="text-muted-foreground">
                  {formatDate(share.created_at)}
                </TableCell>
                <TableCell className="text-muted-foreground">
                  {formatDate(share.last_accessed_at)}
                </TableCell>
                <TableCell className="text-muted-foreground">
                  {share.expires_at
                    ? formatDate(share.expires_at)
                    : m.shares_never_expires()}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <SharesPagination offset={offset} page={page} total={total} />
    </div>
  );
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
    <div className="flex items-center justify-between gap-4">
      <p className="text-muted-foreground text-sm">
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

function createDateFormatter(locale: string) {
  const formatter = new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  });
  return (value: string | null | undefined) =>
    value ? formatter.format(new Date(value)) : "—";
}

/** API 的 `kind` 是英文代碼,顯示前翻成介面語言;未知值原樣顯示,後端加新類型時不會空白。 */
function shareKindLabel(kind: string): string {
  switch (kind) {
    case "text": {
      return m.shares_kind_text();
    }
    case "file": {
      return m.shares_kind_file();
    }
    case "link": {
      return m.shares_kind_link();
    }
    default: {
      return kind;
    }
  }
}
