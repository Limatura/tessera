import { createRoute } from "@tanstack/react-router";
import { Badge } from "@tessera/ui/components/badge";
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

import { AdminShell } from "@/components/admin-shell";
import { useMyShares } from "@/hooks/use-admin";
import { requireAuth } from "@/lib/route-guard";

import { Route as rootRoute } from "./__root";

/**
 * 權限最少的人的落腳頁 —— 只要登入就進得來，不需要任何權限。
 * 顯示的是自己上傳的東西，不是全站的。
 */
export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: SharesComponent,
  getParentRoute: () => rootRoute,
  path: "/shares",
});

const dateFormatter = new Intl.DateTimeFormat("zh-TW", {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatDate(value: string | null | undefined): string {
  return value ? dateFormatter.format(new Date(value)) : "—";
}

function SharesComponent() {
  const shares = useMyShares();
  const rows = shares.data?.data;

  return (
    <AdminShell title="我的分享">
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>我上傳的分享</CardTitle>
          <CardDescription>
            {shares.data
              ? `共 ${shares.data.meta.total} 筆`
              : "由你的帳號建立的分享"}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {shares.isPending && (
            <p className="text-sm text-muted-foreground">載入中…</p>
          )}
          {shares.isError && (
            <p className="text-sm text-destructive">分享載入失敗</p>
          )}
          {rows?.length === 0 && (
            <p className="text-sm text-muted-foreground">還沒有任何分享。</p>
          )}
          {rows && rows.length > 0 && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>取件碼</TableHead>
                  <TableHead>類型</TableHead>
                  <TableHead>建立時間</TableHead>
                  <TableHead>最後取件</TableHead>
                  <TableHead>到期</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((share) => (
                  <TableRow key={share.id}>
                    <TableCell className="font-mono font-medium">
                      {share.code}
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{share.kind}</Badge>
                      {share.has_password && (
                        <Badge className="ml-1" variant="outline">
                          需密碼
                        </Badge>
                      )}
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
                        : "不過期"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </AdminShell>
  );
}
