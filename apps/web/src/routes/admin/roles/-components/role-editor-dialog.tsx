import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldGroup, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";

import { ByteLimitField } from "@/components/admin/byte-limit-field";
import {
  defaultRoleDescription,
  permissionLabel,
  roleLabel,
} from "@/lib/admin/permissions";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

import { PermissionList, PermissionPicker } from "./permission-picker";
import { useRoleForm } from "./use-role-form";

type Role = components["schemas"]["RoleResponse"];
type PermissionInfo = components["schemas"]["PermissionResponse"];

export function RoleEditorDialog({
  role,
  open,
  onOpenChange,
  allPermissions,
  grantable,
}: {
  /** `null` means creating a new role. */
  role: Role | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  allPermissions: PermissionInfo[];
  /** Permissions the signed-in user holds; the server refuses to grant any they lack. */
  grantable: ReadonlySet<string>;
}) {
  const close = () => onOpenChange(false);

  return (
    <ResponsiveDialog onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent className="flex max-h-[85svh] flex-col overflow-hidden sm:max-w-2xl">
        {/* The key remounts the form when the role changes, so the initial values follow. */}
        {open && (
          <RoleEditorForm
            allPermissions={allPermissions}
            grantable={grantable}
            key={role?.id ?? "new"}
            onCancel={close}
            onDone={close}
            role={role}
          />
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

function RoleEditorForm({
  role,
  allPermissions,
  grantable,
  onDone,
  onCancel,
}: {
  role: Role | null;
  allPermissions: PermissionInfo[];
  grantable: ReadonlySet<string>;
  onDone: () => void;
  onCancel: () => void;
}) {
  const form = useRoleForm({ grantable, onDone, role });

  return (
    <>
      <ResponsiveDialogHeader className="shrink-0">
        <ResponsiveDialogTitle>
          {role
            ? m.roles_editor_edit_title({ name: roleLabel(role.name) })
            : m.roles_editor_add_title()}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {form.isSystem
            ? m.roles_editor_description_system()
            : m.roles_editor_description_custom()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>

      <form
        className="min-h-0 flex-1 overflow-y-auto"
        id="role-editor-form"
        onSubmit={form.submit}
      >
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="role-name">
              {m.roles_editor_name_label()}
            </FieldLabel>
            <Input
              id="role-name"
              onChange={(event) => form.setName(event.target.value)}
              required
              value={form.name}
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="role-description">
              {m.roles_editor_description_label()}
            </FieldLabel>
            <Input
              id="role-description"
              onChange={(event) => form.setDescription(event.target.value)}
              placeholder={
                (role && defaultRoleDescription(role)) ??
                m.roles_editor_description_placeholder()
              }
              value={form.description}
            />
          </Field>

          {/* The two quota fields side by side, stacked on phones */}
          <div className="grid gap-4 sm:grid-cols-2">
            <ByteLimitField
              disabled={form.pending}
              id="role-quota"
              inheritLabel={m.roles_editor_quota_inherit()}
              label={m.roles_editor_quota_label()}
              mib={form.quotaMib}
              mode={form.quotaMode}
              onMibChange={form.setQuotaMib}
              onModeChange={form.setQuotaMode}
            />

            <ByteLimitField
              disabled={form.pending}
              id="role-max-file"
              inheritLabel={m.roles_editor_quota_inherit()}
              label={m.roles_editor_file_limit_label()}
              mib={form.fileMib}
              mode={form.fileMode}
              onMibChange={form.setFileMib}
              onModeChange={form.setFileMode}
            />
          </div>

          <Field>
            <FieldLabel>{m.roles_editor_permissions_label()}</FieldLabel>
            {form.isSystem ? (
              <PermissionList
                permissions={allPermissions
                  .map((permission) => permission.name)
                  .filter((permission) => form.selected.has(permission))}
              />
            ) : (
              <PermissionPicker
                allPermissions={allPermissions}
                disabled={form.pending}
                grantable={grantable}
                onToggle={form.toggle}
                selected={form.selected}
              />
            )}
          </Field>

          {form.blocked && (
            <p className="text-destructive text-sm" role="alert">
              {m.roles_editor_blocked_warning()}
              {form.ungrantable.map((permission) => (
                <Badge className="mx-0.5" key={permission} variant="outline">
                  {permissionLabel(permission)}
                </Badge>
              ))}
              {m.roles_editor_blocked_warning_suffix()}
            </p>
          )}

          {form.error && (
            <p className="text-destructive text-sm" role="alert">
              {apiErrorMessage(form.error, m.roles_editor_save_error())}
            </p>
          )}
        </FieldGroup>
      </form>

      <ResponsiveDialogFooter className="shrink-0">
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={form.pending || form.blocked || form.name.trim() === ""}
          form="role-editor-form"
          type="submit"
        >
          {form.pending ? m.common_saving() : m.common_save()}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}
