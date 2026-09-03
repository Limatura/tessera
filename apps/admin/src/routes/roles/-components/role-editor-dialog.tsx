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
          {role ? `編輯「${roleLabel(role.name)}」` : "新增角色"}
        </DialogTitle>
        <DialogDescription>
          {form.isSystem
            ? "內建角色的名稱與配額可以改，權限不行 —— 要讓一般帳號能做的事變少，請自己開一個角色。"
            : "權限是送出整組，不是只送改動的部分。"}
        </DialogDescription>
      </DialogHeader>

      <form
        className="min-h-0 flex-1 overflow-y-auto"
        id="role-editor-form"
        onSubmit={form.submit}
      >
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="role-name">名稱</FieldLabel>
            <Input
              id="role-name"
              onChange={(event) => form.setName(event.target.value)}
              required
              value={form.name}
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="role-description">說明</FieldLabel>
            <Input
              id="role-description"
              onChange={(event) => form.setDescription(event.target.value)}
              placeholder="這個角色是給誰用的"
              value={form.description}
            />
          </Field>

          {/* 兩個配額欄位並排，手機自動堆疊 */}
          <div className="grid gap-4 sm:grid-cols-2">
            <ByteLimitField
              disabled={form.pending}
              id="role-quota"
              inheritLabel="跟隨全站上限"
              label="每個帳號的儲存配額"
              mib={form.quotaMib}
              mode={form.quotaMode}
              onMibChange={form.setQuotaMib}
              onModeChange={form.setQuotaMode}
            />

            <ByteLimitField
              disabled={form.pending}
              id="role-max-file"
              inheritLabel="跟隨全站上限"
              label="單檔大小上限"
              mib={form.fileMib}
              mode={form.fileMode}
              onMibChange={form.setFileMib}
              onModeChange={form.setFileMode}
            />
          </div>

          <Field>
            <FieldLabel>權限</FieldLabel>
            {form.isSystem && (
              <FieldDescription>這是內建角色，權限不可更動。</FieldDescription>
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
              這個角色帶有你自己沒有的權限（
              {form.ungrantable.map((permission) => (
                <Badge className="mx-0.5" key={permission} variant="outline">
                  {permission}
                </Badge>
              ))}
              ），儲存會被拒絕。
            </p>
          )}

          {form.error && (
            <p className="text-destructive text-sm" role="alert">
              {form.error.message ?? "儲存失敗"}
            </p>
          )}
        </FieldGroup>
      </form>

      <DialogFooter className="shrink-0">
        <Button onClick={onCancel} type="button" variant="outline">
          取消
        </Button>
        <Button
          disabled={form.pending || form.blocked || form.name.trim() === ""}
          form="role-editor-form"
          type="submit"
        >
          {form.pending ? "儲存中…" : "儲存"}
        </Button>
      </DialogFooter>
    </>
  );
}
