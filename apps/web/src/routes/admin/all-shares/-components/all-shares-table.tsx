import { IconTrash } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { formatBytes } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";

import { PurgeDate } from "@/components/admin/purge-date";
import { ShareStatusBadge } from "@/components/admin/share-status-badge";
import { SelectAllCheckbox } from "@/components/selection-bar";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { RowSelection } from "@/hooks/use-row-selection";
import {
  isDeadShare,
  ownerLabel,
  shareLabel,
  sizeLabel,
} from "@/lib/admin/admin-shares";
import type { AdminShare } from "@/lib/admin/admin-shares";
import * as m from "@/paraglide/messages";

/**
 * The wide-screen layout. Seven columns do not fit on a phone; narrow screens use
 * `AllSharesList`. Accounts that cannot delete get a `null` `selection`, and the
 * checkbox and delete columns disappear.
 *
 * Expired and used-up rows fade to the secondary color: this page is for finding what
 * to clean up, so live shares and leftover records must stand apart at a glance. Rows
 * disabled by the malware scanner do not fade, since they need attention.
 */
export function AllSharesTable({
  shares,
  selection,
  onDelete,
}: {
  shares: AdminShare[];
  selection: RowSelection | null;
  onDelete: (share: AdminShare) => void;
}) {
  const formatDate = useDateTimeFormatter();

  return (
    <Table>
      <TableHeader>
        <TableRow>
          {selection && (
            <TableHead className="w-10">
              <SelectAllCheckbox selection={selection} />
            </TableHead>
          )}
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
          {selection && <TableHead className="w-12" />}
        </TableRow>
      </TableHeader>
      <TableBody>
        {shares.map((share) => {
          const label = shareLabel(share);
          const selected = selection?.isSelected(share.id) ?? false;
          const dead = isDeadShare(share);
          return (
            <TableRow
              className={cn(dead && "text-muted-foreground")}
              data-state={selected ? "selected" : undefined}
              key={share.id}
            >
              {selection && (
                <TableCell>
                  <Checkbox
                    aria-label={m.selection_select_row({ label })}
                    checked={selected}
                    onCheckedChange={(checked) =>
                      selection.setOne(share.id, checked)
                    }
                  />
                </TableCell>
              )}
              <TableCell
                className={cn("max-w-64 truncate", !dead && "font-medium")}
              >
                {label}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {ownerLabel(share)}
              </TableCell>
              <TableCell
                className={cn(
                  "text-right tabular-nums",
                  share.content_removed && "text-muted-foreground"
                )}
              >
                {sizeLabel(share)}
              </TableCell>
              <TableCell className="text-right tabular-nums text-muted-foreground">
                {formatBytes(share.bytes_served)}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {formatDate(share.created_at)}
              </TableCell>
              <TableCell>
                <span className="flex flex-col items-start gap-1">
                  <ShareStatusBadge share={share} />
                  <PurgeDate share={share} />
                </span>
              </TableCell>
              {selection && (
                <TableCell>
                  <Button
                    aria-label={m.all_shares_delete_aria({ label })}
                    onClick={() => onDelete(share)}
                    size="icon"
                    type="button"
                    variant="ghost"
                  >
                    <IconTrash />
                  </Button>
                </TableCell>
              )}
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
