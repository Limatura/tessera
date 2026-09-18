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

import { useSetUserRole } from "@/hooks/use-admin";
import { roleLabel, UNASSIGNED_ROLE_LABEL } from "@/lib/permissions";

type Account = components["schemas"]["AccountResponse"];
type Role = components["schemas"]["RoleResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/** 選單裡代表「未指派」的值，因為 Select 不接受空字串或 null 當 item value。 */
const UNASSIGNED_VALUE = "__unassigned__";

export function SetUserRoleDialog({
  account,
  roles,
  open,
  onOpenChange,
}: {
  account: Account | null;
  roles: Role[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
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
            roles={roles}
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

  const currentRole = account.roles[0] ?? null;
  const [role, setRole] = useState<string | null>(currentRole);

  const onSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    setUserRole.mutate({ id: account.id, role }, { onSuccess: onDone });
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>設定「{account.username}」的角色</DialogTitle>
        <DialogDescription>
          一個帳號只會有一個角色。選「未指派」代表這個帳號登入得了，但什麼都不能做。
        </DialogDescription>
      </DialogHeader>

      <form id="set-user-role-form" onSubmit={onSubmit}>
        <Field>
          <FieldLabel htmlFor="user-role">角色</FieldLabel>
          <Select
            onValueChange={(next) =>
              setRole(next === UNASSIGNED_VALUE ? null : next)
            }
            value={role ?? UNASSIGNED_VALUE}
          >
            <SelectTrigger id="user-role">
              <SelectValue placeholder="選擇角色" />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value={UNASSIGNED_VALUE}>
                  {UNASSIGNED_ROLE_LABEL}
                </SelectItem>
                {roles.map((r) => (
                  <SelectItem key={r.id} value={r.name}>
                    {roleLabel(r.name)}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>

        {error && (
          <p className="mt-2 text-destructive text-sm" role="alert">
            {error.message ?? "設定失敗"}
          </p>
        )}
      </form>

      <DialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          取消
        </Button>
        <Button
          disabled={setUserRole.isPending || role === currentRole}
          form="set-user-role-form"
          type="submit"
        >
          {setUserRole.isPending ? "儲存中…" : "儲存"}
        </Button>
      </DialogFooter>
    </>
  );
}
