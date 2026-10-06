import { IconTrash } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { useState } from "react";

import { AdminShell } from "@/components/admin/admin-shell";
import { Pager, pageSearchSchema } from "@/components/admin/pager";
import { ShareStatusBadge } from "@/components/admin/share-status-badge";
import { PAGE_SIZE, useAllShares } from "@/hooks/admin/use-admin";
import { useSession } from "@/hooks/use-auth";
import { ownerLabel, shareLabel } from "@/lib/admin/admin-shares";
import type { AdminShare } from "@/lib/admin/admin-shares";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import { formatBytes } from "@/lib/format";
import * as m from "@/paraglide/messages";

import { DeleteShareDialog } from "./all-shares/-components/delete-share-dialog";

/**
 * 全站的分享,用來找出該清掉的東西。跟「我的分享」不同:這裡看不到取件碼 ——
 * 能讀每一份的取件碼等於能打開每一份,那是另一種權力(見 AdminShareResponse)。
 */
export const Route = createFileRoute("/admin/all-shares")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.sharesReadAny
    ),
  component: AllSharesComponent,
  validateSearch: pageSearchSchema,
});

const dateFormatter = new Intl.DateTimeFormat("zh-TW", {
  dateStyle: "medium",
  timeStyle: "short",
});

function AllSharesComponent() {
  const { page } = Route.useSearch();
  const shares = useAllShares((page - 1) * PAGE_SIZE);
  const { data: account } = useSession();
  const [deleting, setDeleting] = useState<AdminShare | null>(null);

  const canDelete = hasPermission(account, PERMISSIONS.sharesDeleteAny);
  const rows = shares.data?.data;

  return (
    <AdminShell title={m.all_shares_page_title()}>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.all_shares_card_title()}</CardTitle>
          <CardDescription>
            {shares.data
              ? m.all_shares_card_description_total({
                  total: shares.data.meta.total,
                })
              : m.all_shares_card_description_empty()}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {shares.isPending && (
            <p className="text-sm text-muted-foreground">
              {m.common_loading()}
            </p>
          )}
          {shares.isError && (
            <p className="text-sm text-destructive">
              {m.all_shares_load_error()}
            </p>
          )}
          {rows?.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {m.all_shares_empty()}
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
                  <TableHead className="text-right">
                    {m.all_shares_col_served()}
                  </TableHead>
                  <TableHead>{m.all_shares_col_created()}</TableHead>
                  <TableHead>{m.all_shares_col_status()}</TableHead>
                  {canDelete && <TableHead className="w-12" />}
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
                    <TableCell className="text-right tabular-nums text-muted-foreground">
                      {formatBytes(share.bytes_served)}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {dateFormatter.format(new Date(share.created_at))}
                    </TableCell>
                    <TableCell>
                      <ShareStatusBadge status={share.status} />
                    </TableCell>
                    {canDelete && (
                      <TableCell>
                        <Button
                          aria-label={m.all_shares_delete_aria({
                            label: shareLabel(share),
                          })}
                          onClick={() => setDeleting(share)}
                          size="icon"
                          type="button"
                          variant="ghost"
                        >
                          <IconTrash />
                        </Button>
                      </TableCell>
                    )}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {shares.data && (
            <Pager
              page={page}
              pageSize={PAGE_SIZE}
              to="/admin/all-shares"
              total={shares.data.meta.total}
            />
          )}
        </CardContent>
      </Card>

      {canDelete && (
        <DeleteShareDialog
          label={deleting ? shareLabel(deleting) : ""}
          onOpenChange={(open) => {
            if (!open) {
              setDeleting(null);
            }
          }}
          share={deleting}
        />
      )}
    </AdminShell>
  );
}
