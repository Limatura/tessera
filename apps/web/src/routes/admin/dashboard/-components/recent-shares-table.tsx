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

import { ListError } from "@/components/admin/list-state";
import { ShareStatusBadge } from "@/components/admin/share-status-badge";
import { useRecentShares } from "@/hooks/admin/use-all-shares";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { ownerLabel, shareLabel } from "@/lib/admin/admin-shares";
import * as m from "@/paraglide/messages";

/**
 * The latest few shares on the instance, from `GET /admin/shares` (needs
 * `shares:read:any`, which the dashboard's parent checks first). Read-only: deleting
 * and paging are on the "All shares" page.
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
          <ListError
            isRetrying={shares.isFetching}
            message={m.all_shares_load_error()}
            onRetry={() => shares.refetch()}
          />
        )}
        {rows?.length === 0 && (
          <p className="text-muted-foreground text-sm">
            {m.dashboard_recent_shares_empty()}
          </p>
        )}
        {rows && rows.length > 0 && (
          <ul className="flex flex-col divide-y md:hidden">
            {rows.map((share) => (
              <li
                className="flex flex-col gap-1 py-3 first:pt-0"
                key={share.id}
              >
                <span className="flex items-center justify-between gap-3">
                  <span className="truncate font-medium text-sm">
                    {shareLabel(share)}
                  </span>
                  <ShareStatusBadge share={share} />
                </span>
                <span className="truncate text-muted-foreground text-xs tabular-nums">
                  {ownerLabel(share)} · {formatBytes(share.size_bytes)} ·{" "}
                  {formatDate(share.created_at)}
                </span>
              </li>
            ))}
          </ul>
        )}
        {rows && rows.length > 0 && (
          <div className="hidden md:block">
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
                      <ShareStatusBadge share={share} />
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(share.created_at)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
