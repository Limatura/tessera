import { IconPencil, IconPlus, IconTrash } from "@tabler/icons-react";
import { createFileRoute } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
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

import { usePermissions, useRoles } from "@/hooks/admin/use-admin";
import { PERMISSIONS, hasPermission, roleLabel } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { DeleteRoleDialog } from "./-components/delete-role-dialog";
import { RoleEditorDialog } from "./-components/role-editor-dialog";

type Role = components["schemas"]["RoleResponse"];

export const Route = createFileRoute("/admin/roles/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.rolesRead
    ),
  component: RolesComponent,
  staticData: { title: m.roles_page_title },
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
    <>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.roles_card_title()}</CardTitle>
          <CardDescription>{m.roles_card_description()}</CardDescription>
          {canWrite && (
            <Button
              className="w-fit"
              onClick={() => openEditor(null)}
              type="button"
            >
              <IconPlus />
              {m.roles_add()}
            </Button>
          )}
        </CardHeader>
        <CardContent>
          {roles.isPending && (
            <p className="text-muted-foreground text-sm">
              {m.common_loading()}
            </p>
          )}
          {roles.isError && (
            <p className="text-destructive text-sm">{m.roles_load_error()}</p>
          )}
          {roles.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{m.roles_col_name()}</TableHead>
                  <TableHead>{m.roles_col_description()}</TableHead>
                  <TableHead className="text-right">
                    {m.roles_col_members()}
                  </TableHead>
                  <TableHead>{m.roles_col_permissions()}</TableHead>
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
                          <Badge variant="outline">
                            {m.roles_builtin_badge()}
                          </Badge>
                        )}
                      </span>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {role.description ?? m.common_dash_placeholder()}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {role.members}
                    </TableCell>
                    <TableCell>
                      {role.permissions.length === 0 ? (
                        <span className="text-muted-foreground">
                          {m.roles_no_permissions()}
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
                            aria-label={m.roles_edit_aria({ name: role.name })}
                            onClick={() => openEditor(role)}
                            size="icon"
                            type="button"
                            variant="ghost"
                          >
                            <IconPencil />
                          </Button>
                          <Button
                            aria-label={m.roles_delete_aria({
                              name: role.name,
                            })}
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
    </>
  );
}
