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

import { ListError, ListSkeleton } from "@/components/admin/list-state";
import { usePermissions, useRoles } from "@/hooks/admin/use-roles";
import {
  HIGH_RISK_PERMISSIONS,
  PERMISSIONS,
  hasPermission,
  permissionLabel,
  roleDescription,
  roleLabel,
} from "@/lib/admin/permissions";
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
          {roles.isPending && <ListSkeleton rows={4} />}
          {roles.isError && (
            <ListError
              isRetrying={roles.isFetching}
              message={m.roles_load_error()}
              onRetry={() => roles.refetch()}
            />
          )}
          {roles.data && (
            <>
              <ul className="flex flex-col gap-3 md:hidden">
                {roles.data.map((role) => (
                  <RoleCard
                    key={role.id}
                    onDelete={canWrite ? () => openDelete(role) : null}
                    onEdit={canWrite ? () => openEditor(role) : null}
                    role={role}
                  />
                ))}
              </ul>
              <div className="hidden md:block">
                <RolesTable
                  onDelete={canWrite ? openDelete : null}
                  onEdit={canWrite ? openEditor : null}
                  roles={roles.data}
                />
              </div>
            </>
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

/** The `md` and up layout. */
function RolesTable({
  roles,
  onEdit,
  onDelete,
}: {
  roles: Role[];
  /** `null`: no `roles:write`, so no actions column. */
  onEdit: ((role: Role) => void) | null;
  onDelete: ((role: Role) => void) | null;
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{m.roles_col_name()}</TableHead>
          <TableHead className="text-right">{m.roles_col_members()}</TableHead>
          <TableHead>{m.roles_col_permissions()}</TableHead>
          {onEdit && <TableHead className="w-24" />}
        </TableRow>
      </TableHeader>
      <TableBody>
        {roles.map((role) => (
          <TableRow key={role.id}>
            {/* The description sits under the name and wraps: as its own
                nowrap column it took the width the permission chips need. */}
            <TableCell className="w-72 whitespace-normal align-top">
              <span className="flex flex-col gap-1">
                <span className="font-medium">
                  <RoleName role={role} />
                </span>
                {roleDescription(role) && (
                  <span className="text-muted-foreground text-xs">
                    {roleDescription(role)}
                  </span>
                )}
              </span>
            </TableCell>
            <TableCell className="text-right align-top tabular-nums">
              {role.members}
            </TableCell>
            <TableCell className="whitespace-normal align-top">
              <PermissionBadges permissions={role.permissions} />
            </TableCell>
            {onEdit && onDelete && (
              <TableCell className="align-top">
                <RoleActions
                  onDelete={() => onDelete(role)}
                  onEdit={() => onEdit(role)}
                  role={role}
                />
              </TableCell>
            )}
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

/** The narrow-screen layout: name and actions on top, description, then what it permits. */
function RoleCard({
  role,
  onEdit,
  onDelete,
}: {
  role: Role;
  onEdit: (() => void) | null;
  onDelete: (() => void) | null;
}) {
  const description = roleDescription(role);

  return (
    <li className="rounded-xl border bg-card">
      <div className="flex items-start gap-3 p-4 pb-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <p className="font-medium text-sm">
            <RoleName role={role} />
          </p>
          {description && (
            <p className="text-muted-foreground text-xs">{description}</p>
          )}
          <p className="text-muted-foreground text-xs tabular-nums">
            {m.roles_member_count({ count: role.members })}
          </p>
        </div>
        {onEdit && onDelete && (
          <div className="-mt-2 -mr-2 shrink-0">
            <RoleActions onDelete={onDelete} onEdit={onEdit} role={role} />
          </div>
        )}
      </div>
      <div className="border-t px-4 py-3">
        <PermissionBadges permissions={role.permissions} />
      </div>
    </li>
  );
}

function RoleName({ role }: { role: Role }) {
  return (
    <span className="flex items-center gap-2">
      {roleLabel(role.name)}
      {role.is_system && (
        <Badge variant="outline">{m.roles_builtin_badge()}</Badge>
      )}
    </span>
  );
}

function PermissionBadges({ permissions }: { permissions: readonly string[] }) {
  if (permissions.length === 0) {
    return (
      <span className="text-muted-foreground text-sm">
        {m.roles_no_permissions()}
      </span>
    );
  }
  return (
    <span className="flex flex-wrap gap-1">
      {permissions.map((permission) => (
        <Badge
          key={permission}
          variant={
            HIGH_RISK_PERMISSIONS.has(permission) ? "destructive" : "secondary"
          }
        >
          {permissionLabel(permission)}
        </Badge>
      ))}
    </span>
  );
}

function RoleActions({
  role,
  onEdit,
  onDelete,
}: {
  role: Role;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <span className="flex justify-end gap-1">
      <Button
        aria-label={m.roles_edit_aria({ name: role.name })}
        className="max-md:size-10"
        onClick={onEdit}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconPencil />
      </Button>
      <Button
        aria-label={m.roles_delete_aria({ name: role.name })}
        className="max-md:size-10"
        disabled={role.is_system}
        onClick={onDelete}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconTrash />
      </Button>
    </span>
  );
}
