import { Link } from "@tanstack/react-router";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import * as m from "@/paraglide/messages";

import { ShareBadges } from "./share-fields";
import type { Share } from "./share-fields";
import { ShareRowActions } from "./share-row-actions";

/** 寬螢幕的版面。五欄在手機上排不下,窄螢幕改用 `SharesList`。 */
export function SharesTable({
  shares,
  onManage,
  onDelete,
}: {
  shares: Share[];
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  const formatDate = useDateTimeFormatter();

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{m.shares_col_code()}</TableHead>
          <TableHead>{m.shares_col_kind()}</TableHead>
          <TableHead>{m.shares_col_created()}</TableHead>
          <TableHead>{m.shares_col_accessed()}</TableHead>
          <TableHead>{m.shares_col_expires()}</TableHead>
          <TableHead className="w-24" />
        </TableRow>
      </TableHeader>
      <TableBody>
        {shares.map((share) => (
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
              <ShareBadges share={share} />
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
            <TableCell>
              <ShareRowActions
                onDelete={onDelete}
                onManage={onManage}
                share={share}
              />
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
