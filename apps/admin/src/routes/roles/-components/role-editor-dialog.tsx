import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";

import { roleLabel } from "@/lib/permissions";
import * as m from "@/paraglide/messages";

import { ByteLimitField } from "./byte-limit-field";
import { PermissionPicker } from "./permission-picker";
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
  /** `null` 代表新增。 */
  role: Role | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  allPermissions: PermissionInfo[];
  /** 目前登入者自己持有的權限 —— server 不允許授出自己沒有的。 */
  grantable: ReadonlySet<string>;
}) {
  const close = () => onOpenChange(false);

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="flex max-h-[85svh] flex-col overflow-hidden sm:max-w-2xl">
        {/* key 換角色時重新掛載表單，初始值才會跟著換。 */}
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
      </DialogContent>
    </Dialog>
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
      <DialogHeader className="shrink-0">
        <DialogTitle>
          {role
            ? m.roles_editor_edit_title({ name: roleLabel(role.name) })
            : m.roles_editor_add_title()}
        </DialogTitle>
        <DialogDescription>
          {form.isSystem
            ? m.roles_editor_description_system()
            : m.roles_editor_description_custom()}
        </DialogDescription>
      </DialogHeader>

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
              placeholder={m.roles_editor_description_placeholder()}
              value={form.description}
            />
          </Field>

          {/* 兩個配額欄位並排，手機自動堆疊 */}
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
            {form.isSystem && (
              <FieldDescription>
                {m.roles_editor_permissions_system_note()}
              </FieldDescription>
            )}
            <PermissionPicker
              allPermissions={allPermissions}
              frozen={form.isSystem || form.pending}
              grantable={grantable}
              onToggle={form.toggle}
              selected={form.selected}
            />
          </Field>

          {form.blocked && (
            <p className="text-destructive text-sm" role="alert">
              {m.roles_editor_blocked_warning()}
              {form.ungrantable.map((permission) => (
                <Badge className="mx-0.5" key={permission} variant="outline">
                  {permission}
                </Badge>
              ))}
              {m.roles_editor_blocked_warning_suffix()}
            </p>
          )}

          {form.error && (
            <p className="text-destructive text-sm" role="alert">
              {form.error.message ?? m.roles_editor_save_error()}
            </p>
          )}
        </FieldGroup>
      </form>

      <DialogFooter className="shrink-0">
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
      </DialogFooter>
    </>
  );
}
