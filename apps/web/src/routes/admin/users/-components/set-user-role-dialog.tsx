import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldLabel } from "@tessera/ui/components/field";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
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

/**
 * The "unassigned" value in the select, since Select accepts neither an empty
 * string nor null as an item value.
 */
const UNASSIGNED_VALUE = "__unassigned__";

/**
 * Roles are fetched here and not on the users page: that needs `roles:read`,
 * and only someone who can change roles mounts this dialog. A custom role with
 * just `users:read` should not get a 403 for opening the users page.
 * Fetched on mount, not on open, because the initial value needs the list to
 * map the role name back to an id.
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
    <ResponsiveDialog onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent className="sm:max-w-md">
        {/* Keyed so switching accounts remounts with new initial values. */}
        {open && account && (
          <SetUserRoleForm
            account={account}
            key={account.id}
            onCancel={close}
            onDone={close}
            roles={roles.data ?? []}
          />
        )}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
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

  // The API takes a role id, but the account carries only the role name, so
  // look the id up in the role list.
  const currentRole =
    roles.find((r) => r.name === account.roles[0])?.id ?? null;
  const [role, setRole] = useState<string | null>(currentRole);

  const onSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    setUserRole.mutate({ id: account.id, role }, { onSuccess: onDone });
  };

  return (
    <>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {m.users_set_role_dialog_title({ name: account.username })}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {m.users_set_role_dialog_description()}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>

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
              {/* The value is an id; the trigger shows the role name. */}
              <SelectValue placeholder={m.users_set_role_dialog_placeholder()}>
                {(value: string) =>
                  value === UNASSIGNED_VALUE
                    ? unassignedRoleLabel()
                    : roleLabel(roles.find((r) => r.id === value)?.name ?? "")
                }
              </SelectValue>
            </SelectTrigger>
            {/* By default the menu overlays the trigger, which inside a dialog
               covers the title; open it downwards. */}
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

      <ResponsiveDialogFooter>
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
      </ResponsiveDialogFooter>
    </>
  );
}
