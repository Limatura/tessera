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

import { DeadSharesNote } from "@/components/dead-shares-note";
import { SelectAllRow, SelectionBar } from "@/components/selection-bar";
import { useRowSelection } from "@/hooks/use-row-selection";
import {
  MY_SHARES_KEY,
  SHARES_PAGE_SIZE,
  useMyShares,
} from "@/hooks/use-shares";
import { uploader } from "@/lib/uploader";
import * as m from "@/paraglide/messages";
import { useShareKeysStore } from "@/stores/share-keys";

import { ManageShareDialog } from "../../-components/manage-share-dialog";
import type { ManagedShare } from "../../-components/manage-share-dialog";
import { DeleteShareDialog } from "./delete-share-dialog";
import { shareState } from "./share-fields";
import type { Share } from "./share-fields";
import { SharesList } from "./shares-list";
import { SharesTable } from "./shares-table";

/**
 * Data and paging for "My shares". The layout depends on width: a list on narrow
 * screens, a table on wide ones.
 *
 * Switched in CSS, not `useIsMobile`: both render and one shows, so the first paint has
 * the right layout instead of flashing the table before the list. A page holds at most
 * 20 rows, so the extra DOM is negligible.
 */
export function MyShares({ page }: { page: number }) {
  const offset = (page - 1) * SHARES_PAGE_SIZE;
  const shares = useMyShares(offset);
  const [deleting, setDeleting] = useState<Share[]>([]);
  const [managing, setManaging] = useState<ManagedShare | null>(null);
  const [manageOpen, setManageOpen] = useState(false);
  const queryClient = useQueryClient();
  const rows = shares.data?.data ?? [];
  const selection = useRowSelection(
    rows.map((share) => share.code),
    page
  );

  // Changing expiry or limits makes the expiry column stale; deleting also
  // changes the total and the paging.
  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: MY_SHARES_KEY });

  const deadCodes = rows
    .filter((share) => shareState(share) !== "live")
    .map((share) => share.code);

  const handleDeleteSelected = () =>
    setDeleting(rows.filter((share) => selection.isSelected(share.code)));

  // Drop deleted ones from the selection and the dialog; what stays in the dialog is what to retry.
  const handleDeleted = (codes: string[]) => {
    selection.deselect(codes);
    setDeleting((current) =>
      current.filter((share) => !codes.includes(share.code))
    );
  };

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

  const { total } = shares.data.meta;

  // Deleting the last row of the last page leaves it empty: go back to the new
  // last page instead of showing "no shares yet".
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
    <div
      className={cn(
        "space-y-4",
        selection.selected.length > 0 && "max-md:pb-20"
      )}
    >
      <SelectionBar
        deadIds={deadCodes}
        onDelete={handleDeleteSelected}
        selection={selection}
      />
      <div className="md:hidden">
        <SelectAllRow deadIds={deadCodes} selection={selection} />
        <SharesList
          onDelete={(share) => setDeleting([share])}
          onManage={handleManage}
          selection={selection}
          shares={rows}
        />
      </div>
      <div className="hidden md:block">
        <SharesTable
          onDelete={(share) => setDeleting([share])}
          onManage={handleManage}
          selection={selection}
          shares={rows}
        />
      </div>

      {deadCodes.length > 0 && (
        <DeadSharesNote>{m.shares_dead_note()}</DeadSharesNote>
      )}

      <SharesPagination offset={offset} page={page} total={total} />

      <DeleteShareDialog
        onDeleted={handleDeleted}
        onOpenChange={(open) => {
          if (!open) {
            setDeleting([]);
          }
        }}
        shares={deleting}
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

/** List data is fresh from the server, so no refetch. Authorised by the session, so no token. */
function toManaged(share: Share): ManagedShare {
  const result = uploader.toResult(share);
  // A link-encrypted share opens only with the key after `#`, which the
  // server never had: it comes from this browser, if it uploaded the share.
  const key =
    share.encryption === "link"
      ? useShareKeysStore.getState().keys[share.code]
      : undefined;
  return {
    code: share.code,
    initial: {
      burnAfterReading: share.uses_remaining === 1,
      expiresInMinutes: result.expiresInMinutes,
      hasPassword: share.has_password,
      passwordIsKey: share.encryption === "password",
    },
    keyMissing: share.encryption === "link" && !key,
    shortUrl: key ? `${result.shortUrl}#${key}` : result.shortUrl,
    title: result.fileName,
  };
}

/**
 * Previous/next are `Link`s, not buttons, so middle-click and copying the link work.
 * At either end they render as a disabled span: a link has no disabled state, only
 * the absence of an href.
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
