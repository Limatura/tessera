import { Link } from "@tanstack/react-router";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Skeleton } from "@tessera/ui/components/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { formatBytes } from "@tessera/ui/lib/format";

import { ShareStatusBadge } from "@/components/admin/share-status-badge";
import { useRecentShares } from "@/hooks/admin/use-admin";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { ownerLabel, shareLabel } from "@/lib/admin/admin-shares";
import * as m from "@/paraglide/messages";

/**
 * 全站最新的幾筆，用 `GET /admin/shares`（要 `shares:read:any`，儀表板的父層會先擋）。
 * 要刪、要翻頁就去「全站分享」，這裡只看。
 */
export function RecentSharesTable() {
  const shares = useRecentShares();
  const formatDate = useDateTimeFormatter();
  const rows = shares.data?.data;

  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_recent_shares_title()}</CardTitle>
        <CardDescription>
          {m.dashboard_recent_shares_description()}
        </CardDescription>
        <CardAction>
          <Link
            className={buttonVariants({ size: "sm", variant: "ghost" })}
            search={{ page: 1 }}
            to="/admin/all-shares"
          >
            {m.dashboard_recent_shares_view_all()}
          </Link>
        </CardAction>
      </CardHeader>
      <CardContent>
        {shares.isPending && <Skeleton className="h-48" />}
        {shares.isError && (
          <p className="text-destructive text-sm">
            {m.all_shares_load_error()}
          </p>
        )}
        {rows?.length === 0 && (
          <p className="text-muted-foreground text-sm">
            {m.dashboard_recent_shares_empty()}
          </p>
        )}
        {rows && rows.length > 0 && (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{m.all_shares_col_content()}</TableHead>
                <TableHead>{m.all_shares_col_owner()}</TableHead>
                <TableHead className="text-right">
                  {m.all_shares_col_size()}
                </TableHead>
                <TableHead>{m.all_shares_col_status()}</TableHead>
                <TableHead>{m.all_shares_col_created()}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((share) => (
                <TableRow key={share.id}>
                  <TableCell className="max-w-64 truncate font-medium">
                    {shareLabel(share)}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {ownerLabel(share)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatBytes(share.size_bytes)}
                  </TableCell>
                  <TableCell>
                    <ShareStatusBadge status={share.status} />
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {formatDate(share.created_at)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
