import { IconPencil } from "@tabler/icons-react";
import { createRoute } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
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
import { useState } from "react";

import { AdminShell } from "@/components/admin-shell";
import { Pager, pageSearchSchema } from "@/components/pager";
import { PAGE_SIZE, useRoles, useUsers } from "@/hooks/use-admin";
import { useSession } from "@/hooks/use-auth";
import {
  accountRoleLabel,
  hasPermission,
  isUnassigned,
  PERMISSIONS,
} from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "./__root";
import { SetUserRoleDialog } from "./users/-components/set-user-role-dialog";

type Account = components["schemas"]["AccountResponse"];

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
  validateSearch: pageSearchSchema,
});

function UsersComponent() {
  const { page } = Route.useSearch();
  const users = useUsers((page - 1) * PAGE_SIZE);
  const roles = useRoles();
  const { data: account } = useSession();

  const [editing, setEditing] = useState<Account | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);

  const canWrite = hasPermission(account, PERMISSIONS.usersWrite);

  const openEditor = (user: Account) => {
    setEditing(user);
    setEditorOpen(true);
  };

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
                  {canWrite && <TableHead className="w-12" />}
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
                    {canWrite && (
                      <TableCell>
                        <Button
                          aria-label={`設定 ${user.username} 的角色`}
                          onClick={() => openEditor(user)}
                          size="icon"
                          type="button"
                          variant="ghost"
                        >
                          <IconPencil />
                        </Button>
                      </TableCell>
                    )}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {users.data && (
            <Pager
              page={page}
              pageSize={PAGE_SIZE}
              to="/users"
              total={users.data.meta.total}
            />
          )}
        </CardContent>
      </Card>

      {canWrite && (
        <SetUserRoleDialog
          account={editing}
          onOpenChange={setEditorOpen}
          open={editorOpen}
          roles={roles.data ?? []}
        />
      )}
    </AdminShell>
  );
}
