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
        <CardTitle>最近分享</CardTitle>
        <CardDescription>最新建立的 10 筆分享</CardDescription>
      </CardHeader>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>內容</TableHead>
            <TableHead>大小</TableHead>
            <TableHead>建立者</TableHead>
            <TableHead>狀態</TableHead>
            <TableHead>建立時間</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.length === 0 && (
            <TableRow>
              <TableCell
                colSpan={5}
                className="h-24 text-center text-muted-foreground"
              >
                目前沒有分享記錄
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
                {row.owner ?? "訪客"}
              </TableCell>
              <TableCell>
                <Badge variant={row.active ? "outline" : "secondary"}>
                  {row.active ? "可取件" : "已失效"}
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
    return "文字分享";
  }
  return row.filename ?? `${row.fileCount} 個檔案`;
}
