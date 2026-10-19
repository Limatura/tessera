import { Link } from "@tanstack/react-router";
import { Checkbox } from "@tessera/ui/components/checkbox";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { cn } from "@tessera/ui/lib/utils";

import { SelectAllCheckbox } from "@/components/selection-bar";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { RowSelection } from "@/hooks/use-row-selection";
import * as m from "@/paraglide/messages";

import {
  ShareBadges,
  ShareStateBadge,
  shareName,
  shareState,
} from "./share-fields";
import type { Share } from "./share-fields";
import { ShareRowActions } from "./share-row-actions";

/**
 * The wide-screen layout. Five columns do not fit on a phone; narrow screens use
 * `SharesList`.
 *
 * Dead rows fade to the secondary color and the expiry column shows why: they are only
 * records now and must stand apart from live ones at a glance. The checkbox and delete
 * button do not fade, since those are what one acts on.
 */
export function SharesTable({
  shares,
  selection,
  onManage,
  onDelete,
}: {
  shares: Share[];
  selection: RowSelection;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  const formatDate = useDateTimeFormatter();

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-10">
            <SelectAllCheckbox selection={selection} />
          </TableHead>
          <TableHead>{m.shares_col_name()}</TableHead>
          <TableHead>{m.shares_col_kind()}</TableHead>
          <TableHead>{m.shares_col_created()}</TableHead>
          <TableHead>{m.shares_col_accessed()}</TableHead>
          <TableHead>{m.shares_col_expires()}</TableHead>
          <TableHead className="w-24" />
        </TableRow>
      </TableHeader>
      <TableBody>
        {shares.map((share) => {
          const state = shareState(share);
          const dead = state !== "live";
          const name = shareName(share);
          return (
            <TableRow
              className={cn(dead && "text-muted-foreground")}
              data-state={
                selection.isSelected(share.code) ? "selected" : undefined
              }
              key={share.id}
            >
              <TableCell>
                <Checkbox
                  aria-label={m.selection_select_row({ label: share.code })}
                  checked={selection.isSelected(share.code)}
                  onCheckedChange={(checked) =>
                    selection.setOne(share.code, checked)
                  }
                />
              </TableCell>
              <TableCell className="max-w-72">
                <Link
                  className="group flex flex-col"
                  params={{ code: share.code }}
                  title={name ?? undefined}
                  to="/s/$code"
                >
                  {name && (
                    <span
                      className={cn(
                        "truncate group-hover:underline",
                        !dead && "font-medium"
                      )}
                    >
                      {name}
                    </span>
                  )}
                  <span
                    className={cn(
                      "font-mono",
                      name
                        ? "text-muted-foreground text-xs"
                        : "group-hover:underline",
                      !(name || dead) && "font-medium"
                    )}
                  >
                    {share.code}
                  </span>
                </Link>
              </TableCell>
              <TableCell className={cn(dead && "opacity-60")}>
                <ShareBadges share={share} />
              </TableCell>
              <TableCell className="text-muted-foreground">
                {formatDate(share.created_at)}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {formatDate(share.last_accessed_at)}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {dead ? (
                  <span className="flex items-center gap-2">
                    <ShareStateBadge state={state} />
                    {state === "expired" && share.expires_at && (
                      <span className="text-xs tabular-nums">
                        {formatDate(share.expires_at)}
                      </span>
                    )}
                  </span>
                ) : (
                  ((share.expires_at && formatDate(share.expires_at)) ??
                  m.shares_never_expires())
                )}
              </TableCell>
              <TableCell>
                <ShareRowActions
                  onDelete={onDelete}
                  onManage={onManage}
                  share={share}
                />
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
