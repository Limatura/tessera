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

import { ShareBadges, ShareStateBadge, shareState } from "./share-fields";
import type { Share } from "./share-fields";
import { ShareRowActions } from "./share-row-actions";

/**
 * 寬螢幕的版面。五欄在手機上排不下,窄螢幕改用 `SharesList`。
 *
 * 已失效的那幾列整列退成次要色,「到期」欄換成失效原因 —— 它們還在列表上只是紀錄,
 * 跟還能取的混在一起時一眼要分得出來。勾選框跟刪除不退色,那是要對它們做的事。
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
          <TableHead>{m.shares_col_code()}</TableHead>
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
              <TableCell className={cn("font-mono", !dead && "font-medium")}>
                <Link
                  className="hover:underline"
                  params={{ code: share.code }}
                  to="/s/$code"
                >
                  {share.code}
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
