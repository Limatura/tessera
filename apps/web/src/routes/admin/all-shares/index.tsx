import { createFileRoute, Navigate } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { cn } from "@tessera/ui/lib/utils";
import { useState } from "react";

import { Pager } from "@/components/admin/pager";
import { DeadSharesNote } from "@/components/dead-shares-note";
import { SelectAllRow, SelectionBar } from "@/components/selection-bar";
import { useAllShares } from "@/hooks/admin/use-all-shares";
import { useRowSelection } from "@/hooks/use-row-selection";
import { isDeadShare } from "@/lib/admin/admin-shares";
import type { AdminShare } from "@/lib/admin/admin-shares";
import { PAGE_SIZE, pageSearchSchema } from "@/lib/admin/page-search";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { AllSharesList } from "./-components/all-shares-list";
import { AllSharesTable } from "./-components/all-shares-table";
import { DeleteShareDialog } from "./-components/delete-share-dialog";

/**
 * 全站的分享,用來找出該清掉的東西。跟「我的分享」不同:這裡看不到取件碼 ——
 * 能讀每一份的取件碼等於能打開每一份,那是另一種權力(見 AdminShareResponse)。
 */
export const Route = createFileRoute("/admin/all-shares/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.sharesReadAny
    ),
  component: AllSharesComponent,
  staticData: { title: m.all_shares_page_title },
  validateSearch: pageSearchSchema,
});

function AllSharesComponent() {
  const { page } = Route.useSearch();
  const shares = useAllShares((page - 1) * PAGE_SIZE);
  const { data: account } = useSession();
  const [deleting, setDeleting] = useState<AdminShare[]>([]);

  const canDelete = hasPermission(account, PERMISSIONS.sharesDeleteAny);
  const rows = shares.data?.data ?? [];
  const selection = useRowSelection(
    rows.map((share) => share.id),
    page
  );
  // 不能刪就不給勾:勾了也沒有能做的事。
  const rowSelection = canDelete ? selection : null;

  const deadIds = rows.filter(isDeadShare).map((share) => share.id);

  const handleDeleteSelected = () =>
    setDeleting(rows.filter((share) => selection.isSelected(share.id)));

  // 刪掉的從選取跟對話框裡拿掉;沒刪掉的留著,對話框裡剩下的就是要重試的那幾份。
  const handleDeleted = (ids: string[]) => {
    selection.deselect(ids);
    setDeleting((current) =>
      current.filter((share) => !ids.includes(share.id))
    );
  };

  // 整頁勾起來刪掉,最後一頁就空了 —— 退回現在的最後一頁,而不是顯示「沒有分享」。
  if (shares.data && rows.length === 0 && page > 1) {
    return (
      <Navigate
        replace
        search={{
          page: Math.max(1, Math.ceil(shares.data.meta.total / PAGE_SIZE)),
        }}
        to="/admin/all-shares"
      />
    );
  }

  return (
    <>
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
        <CardContent
          className={cn(
            "space-y-4",
            selection.selected.length > 0 && "max-md:pb-24"
          )}
        >
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
          {shares.data && rows.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {m.all_shares_empty()}
            </p>
          )}
          {rows.length > 0 && (
            <>
              {rowSelection && (
                <SelectionBar
                  deadIds={deadIds}
                  onDelete={handleDeleteSelected}
                  selection={rowSelection}
                />
              )}
              <div className="md:hidden">
                {rowSelection && (
                  <SelectAllRow deadIds={deadIds} selection={rowSelection} />
                )}
                <AllSharesList
                  onDelete={(share) => setDeleting([share])}
                  selection={rowSelection}
                  shares={rows}
                />
              </div>
              <div className="hidden md:block">
                <AllSharesTable
                  onDelete={(share) => setDeleting([share])}
                  selection={rowSelection}
                  shares={rows}
                />
              </div>
            </>
          )}
          {deadIds.length > 0 && (
            <DeadSharesNote>{m.all_shares_dead_note()}</DeadSharesNote>
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
          onDeleted={handleDeleted}
          onOpenChange={(open) => {
            if (!open) {
              setDeleting([]);
            }
          }}
          shares={deleting}
        />
      )}
    </>
  );
}
