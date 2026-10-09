import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@tessera/ui/components/dialog";
import { Field, FieldLabel } from "@tessera/ui/components/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { useState } from "react";

import { useRoles } from "@/hooks/admin/use-roles";
import { useSetUserRole } from "@/hooks/admin/use-users";
import { roleLabel, unassignedRoleLabel } from "@/lib/admin/permissions";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type Account = components["schemas"]["AccountResponse"];
type Role = components["schemas"]["RoleResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/** 選單裡代表「未指派」的值，因為 Select 不接受空字串或 null 當 item value。 */
const UNASSIGNED_VALUE = "__unassigned__";

/**
 * 角色清單在這裡查而不是在使用者頁：查它要 `roles:read`,只有能改角色的人才會掛這個
 * 對話框 —— 只有 `users:read` 的自訂角色打開使用者頁時不該吃一個 403。
 * 掛載就查、不等打開，表單的初始值要靠清單把角色名稱換回 id。
 */
export function SetUserRoleDialog({
  account,
  open,
  onOpenChange,
}: {
  account: Account | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const roles = useRoles();
  const close = () => onOpenChange(false);

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-md">
        {/* key 換帳號時重新掛載，初始值才會跟著換。 */}
        {open && account && (
          <SetUserRoleForm
            account={account}
            key={account.id}
            onCancel={close}
            onDone={close}
            roles={roles.data ?? []}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function SetUserRoleForm({
  account,
  roles,
  onDone,
  onCancel,
}: {
  account: Account;
  roles: Role[];
  onDone: () => void;
  onCancel: () => void;
}) {
  const setUserRole = useSetUserRole();
  const error = setUserRole.error as ErrorBody | null;

  // API 收的是角色 id，但帳號上只帶角色名稱，要從角色清單查回 id（#354）。
  const currentRole =
    roles.find((r) => r.name === account.roles[0])?.id ?? null;
  const [role, setRole] = useState<string | null>(currentRole);

  const onSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    setUserRole.mutate({ id: account.id, role }, { onSuccess: onDone });
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>
          {m.users_set_role_dialog_title({ name: account.username })}
        </DialogTitle>
        <DialogDescription>
          {m.users_set_role_dialog_description()}
        </DialogDescription>
      </DialogHeader>

      <form id="set-user-role-form" onSubmit={onSubmit}>
        <Field>
          <FieldLabel htmlFor="user-role">
            {m.users_set_role_dialog_role_label()}
          </FieldLabel>
          <Select
            onValueChange={(next) =>
              setRole(next === UNASSIGNED_VALUE ? null : next)
            }
            value={role ?? UNASSIGNED_VALUE}
          >
            <SelectTrigger id="user-role">
              {/* value 現在是 id，觸發按鈕要換回角色名稱來顯示。 */}
              <SelectValue placeholder={m.users_set_role_dialog_placeholder()}>
                {(value: string) =>
                  value === UNASSIGNED_VALUE
                    ? unassignedRoleLabel()
                    : roleLabel(roles.find((r) => r.id === value)?.name ?? "")
                }
              </SelectValue>
            </SelectTrigger>
            {/* 預設會把選單疊在觸發按鈕上，在對話框裡會蓋住標題（#351），改成往下展開。 */}
            <SelectContent alignItemWithTrigger={false}>
              <SelectGroup>
                <SelectItem value={UNASSIGNED_VALUE}>
                  {unassignedRoleLabel()}
                </SelectItem>
                {roles.map((r) => (
                  <SelectItem key={r.id} value={r.id}>
                    {roleLabel(r.name)}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>

        {error && (
          <p className="mt-2 text-destructive text-sm" role="alert">
            {apiErrorMessage(error, m.users_set_role_dialog_error())}
          </p>
        )}
      </form>

      <DialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={setUserRole.isPending || role === currentRole}
          form="set-user-role-form"
          type="submit"
        >
          {setUserRole.isPending ? m.common_saving() : m.common_save()}
        </Button>
      </DialogFooter>
    </>
  );
}
