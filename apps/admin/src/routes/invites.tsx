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
import { useInvites } from "@/hooks/use-admin";
import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.invitesRead
    ),
  component: InvitesComponent,
  getParentRoute: () => rootRoute,
  path: "/invites",
});

const dateFormatter = new Intl.DateTimeFormat("zh-TW", {
  dateStyle: "medium",
  timeStyle: "short",
});

function formatDate(value: string | null | undefined): string {
  return value ? dateFormatter.format(new Date(value)) : "不過期";
}

function InvitesComponent() {
  const invites = useInvites();

  return (
    <AdminShell title="邀請碼">
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>邀請碼</CardTitle>
          <CardDescription>
            是否仍可使用由 server 判斷，不是從次數與時間自己推算的。
          </CardDescription>
        </CardHeader>
        <CardContent>
          {invites.isPending && (
            <p className="text-sm text-muted-foreground">載入中…</p>
          )}
          {invites.isError && (
            <p className="text-sm text-destructive">邀請碼載入失敗</p>
          )}
          {invites.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>備註</TableHead>
                  <TableHead>狀態</TableHead>
                  <TableHead className="text-right">已用 / 上限</TableHead>
                  <TableHead>到期</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {invites.data.map((invite) => (
                  <TableRow key={invite.id}>
                    <TableCell className="font-medium">
                      {invite.note ?? "—"}
                    </TableCell>
                    <TableCell>
                      <Badge variant={invite.active ? "secondary" : "outline"}>
                        {invite.active ? "可使用" : "已失效"}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {invite.used} / {invite.max_uses}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(invite.expires_at)}
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
