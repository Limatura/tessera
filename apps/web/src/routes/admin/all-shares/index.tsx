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

import {
  ListEmpty,
  ListError,
  ListSkeleton,
} from "@/components/admin/list-state";
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
 * Every share on the instance, for finding what to clean up. Unlike "My shares", codes
 * are not shown: reading every share's code means being able to open every share, and
 * that is a different power (see AdminShareResponse).
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
  // No delete permission, no checkboxes: there would be nothing to do with a selection.
  const rowSelection = canDelete ? selection : null;

  const deadIds = rows.filter(isDeadShare).map((share) => share.id);

  const handleDeleteSelected = () =>
    setDeleting(rows.filter((share) => selection.isSelected(share.id)));

  // Drop deleted ones from the selection and the dialog; what stays in the dialog is what to retry.
  const handleDeleted = (ids: string[]) => {
    selection.deselect(ids);
    setDeleting((current) =>
      current.filter((share) => !ids.includes(share.id))
    );
  };

  // Deleting every row on the last page empties it: go back to the new last
  // page instead of showing "no shares".
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
          {shares.isPending && <ListSkeleton />}
          {shares.isError && (
            <ListError
              isRetrying={shares.isFetching}
              message={m.all_shares_load_error()}
              onRetry={() => shares.refetch()}
            />
          )}
          {shares.data && rows.length === 0 && (
            <ListEmpty>{m.all_shares_empty()}</ListEmpty>
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
