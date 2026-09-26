import { Badge } from "@tessera/ui/components/badge";
import {
  Card,
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

import mockData from "@/data/dashboard-mock.json";
import { formatBytes } from "@/lib/format";
import * as m from "@/paraglide/messages";

/**
 * Mirrors the server's `AdminShareResponse` (apps/server/src/api/admin.rs).
 * Backed by static mock data until GET /admin/shares?limit=… is wired up.
 */
interface ShareRow {
  id: string;
  kind: "file" | "text";
  filename: string | null;
  fileCount: number;
  sizeBytes: number;
  owner: string | null;
  active: boolean;
  createdAt: string;
}

const rows: ShareRow[] = mockData.recentShares as ShareRow[];

export function RecentSharesTable() {
  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_recent_shares_title()}</CardTitle>
        <CardDescription>
          {m.dashboard_recent_shares_description()}
        </CardDescription>
      </CardHeader>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{m.dashboard_recent_shares_column_content()}</TableHead>
            <TableHead>{m.dashboard_recent_shares_column_size()}</TableHead>
            <TableHead>{m.dashboard_recent_shares_column_owner()}</TableHead>
            <TableHead>{m.dashboard_recent_shares_column_status()}</TableHead>
            <TableHead>
              {m.dashboard_recent_shares_column_created_at()}
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.length === 0 && (
            <TableRow>
              <TableCell
                colSpan={5}
                className="h-24 text-center text-muted-foreground"
              >
                {m.dashboard_recent_shares_empty()}
              </TableCell>
            </TableRow>
          )}
          {rows.map((row) => (
            <TableRow key={row.id}>
              <TableCell className="max-w-48 truncate font-medium">
                {shareLabel(row)}
              </TableCell>
              <TableCell className="tabular-nums text-muted-foreground">
                {formatBytes(row.sizeBytes)}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {row.owner ?? m.dashboard_recent_shares_guest()}
              </TableCell>
              <TableCell>
                <Badge variant={row.active ? "outline" : "secondary"}>
                  {row.active
                    ? m.dashboard_recent_shares_active()
                    : m.dashboard_recent_shares_inactive()}
                </Badge>
              </TableCell>
              <TableCell className="text-muted-foreground">
                {row.createdAt}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Card>
  );
}

function shareLabel(row: ShareRow) {
  if (row.kind === "text") {
    return m.dashboard_recent_shares_text_share();
  }
  return (
    row.filename ??
    m.dashboard_recent_shares_file_count({ count: row.fileCount })
  );
}
