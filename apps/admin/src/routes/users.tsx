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
import { useUsers } from "@/hooks/use-admin";
import { PERMISSIONS, accountRoleLabel, isUnassigned } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.usersRead
    ),
  component: UsersComponent,
  getParentRoute: () => rootRoute,
  path: "/users",
});

function UsersComponent() {
  const users = useUsers();

  return (
    <AdminShell title="使用者">
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>帳號</CardTitle>
          <CardDescription>
            「未指派」代表還沒有人決定這個帳號能做什麼，跟刻意給訪客角色不同。
          </CardDescription>
        </CardHeader>
        <CardContent>
          {users.isPending && (
            <p className="text-sm text-muted-foreground">載入中…</p>
          )}
          {users.isError && (
            <p className="text-sm text-destructive">帳號載入失敗</p>
          )}
          {users.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>帳號</TableHead>
                  <TableHead>電子郵件</TableHead>
                  <TableHead>角色</TableHead>
                  <TableHead className="text-right">權限數</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.data.data.map((user) => (
                  <TableRow key={user.id}>
                    <TableCell className="font-medium">
                      {user.username}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {user.email}
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={isUnassigned(user) ? "outline" : "secondary"}
                      >
                        {accountRoleLabel(user)}
                      </Badge>
                      {isUnassigned(user) && (
                        <span className="ml-2 text-xs text-muted-foreground">
                          尚未決定能做什麼
                        </span>
                      )}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {user.permissions.length}
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
