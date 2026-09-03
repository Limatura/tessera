import { IconPencil, IconPlus, IconTrash } from "@tabler/icons-react";
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
import { usePermissions, useRoles } from "@/hooks/use-admin";
import { useSession } from "@/hooks/use-auth";
import { PERMISSIONS, hasPermission, roleLabel } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "../__root";
import { DeleteRoleDialog } from "./-components/delete-role-dialog";
import { RoleEditorDialog } from "./-components/role-editor-dialog";

type Role = components["schemas"]["RoleResponse"];

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.rolesRead
    ),
  component: RolesComponent,
  getParentRoute: () => rootRoute,
  path: "/roles",
});

function RolesComponent() {
  const roles = useRoles();
  const permissions = usePermissions();
  const { data: account } = useSession();

  const [editing, setEditing] = useState<Role | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [deleting, setDeleting] = useState<Role | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const canWrite = hasPermission(account, PERMISSIONS.rolesWrite);
  const grantable = new Set(account?.permissions);
  const privileged = new Set(
    permissions.data?.filter((p) => p.privileged).map((p) => p.name)
  );

  const openEditor = (role: Role | null) => {
    setEditing(role);
    setEditorOpen(true);
  };

  const openDelete = (role: Role) => {
    setDeleting(role);
    setDeleteOpen(true);
  };

  return (
    <AdminShell title="角色與權限">
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>角色</CardTitle>
          <CardDescription>
            權限是程式決定的固定清單，角色才是可以增減的。內建角色不可刪除，也不能改權限。
          </CardDescription>
          {canWrite && (
            <Button
              className="w-fit"
              onClick={() => openEditor(null)}
              type="button"
            >
              <IconPlus />
              新增角色
            </Button>
          )}
        </CardHeader>
        <CardContent>
          {roles.isPending && (
            <p className="text-muted-foreground text-sm">載入中…</p>
          )}
          {roles.isError && (
            <p className="text-destructive text-sm">角色載入失敗</p>
          )}
          {roles.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>名稱</TableHead>
                  <TableHead>說明</TableHead>
                  <TableHead className="text-right">成員</TableHead>
                  <TableHead>權限</TableHead>
                  {canWrite && <TableHead className="w-24" />}
                </TableRow>
              </TableHeader>
              <TableBody>
                {roles.data.map((role) => (
                  <TableRow key={role.id}>
                    <TableCell className="font-medium">
                      <span className="flex items-center gap-2">
                        {roleLabel(role.name)}
                        {role.is_system && (
                          <Badge variant="outline">內建</Badge>
                        )}
                      </span>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {role.description ?? "—"}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {role.members}
                    </TableCell>
                    <TableCell>
                      {role.permissions.length === 0 ? (
                        <span className="text-muted-foreground">
                          不允許任何操作
                        </span>
                      ) : (
                        <span className="flex flex-wrap gap-1">
                          {role.permissions.map((permission) => (
                            <Badge
                              key={permission}
                              variant={
                                privileged.has(permission)
                                  ? "destructive"
                                  : "secondary"
                              }
                            >
                              {permission}
                            </Badge>
                          ))}
                        </span>
                      )}
                    </TableCell>
                    {canWrite && (
                      <TableCell>
                        <span className="flex justify-end gap-1">
                          <Button
                            aria-label={`編輯 ${role.name}`}
                            onClick={() => openEditor(role)}
                            size="icon"
                            type="button"
                            variant="ghost"
                          >
                            <IconPencil />
                          </Button>
                          <Button
                            aria-label={`刪除 ${role.name}`}
                            disabled={role.is_system}
                            onClick={() => openDelete(role)}
                            size="icon"
                            type="button"
                            variant="ghost"
                          >
                            <IconTrash />
                          </Button>
                        </span>
                      </TableCell>
                    )}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {canWrite && (
        <RoleEditorDialog
          allPermissions={permissions.data ?? []}
          grantable={grantable}
          onOpenChange={setEditorOpen}
          open={editorOpen}
          role={editing}
        />
      )}
      {canWrite && (
        <DeleteRoleDialog
          onOpenChange={setDeleteOpen}
          open={deleteOpen}
          role={deleting}
        />
      )}
    </AdminShell>
  );
}
