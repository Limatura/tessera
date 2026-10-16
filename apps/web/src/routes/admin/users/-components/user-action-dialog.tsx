import type { components } from "@tessera/api/schema";
import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { toast } from "@tessera/ui/components/toast";

import {
  useDeleteUser,
  useRemoveUserShares,
  useResetUserMfa,
  useSetUserState,
} from "@/hooks/admin/use-users";
import type { AdminUser } from "@/lib/admin/accounts";
import { isDeletedAccount } from "@/lib/admin/accounts";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type Account = AdminUser;
type ErrorBody = components["schemas"]["ErrorBody"];

/** Actions that need confirmation. Re-enabling is a direct click and is not here. */
export type UserAction = "delete" | "disable" | "removeShares" | "resetMfa";

function buildCopy(): Record<
  UserAction,
  { title: (name: string) => string; description: string; confirm: string }
> {
  return {
    delete: {
      confirm: m.users_action_delete_confirm(),
      description: m.users_action_delete_description(),
      title: (name) => m.users_action_delete_title({ name }),
    },
    disable: {
      confirm: m.users_action_disable_confirm(),
      description: m.users_action_disable_description(),
      title: (name) => m.users_action_disable_title({ name }),
    },
    resetMfa: {
      confirm: m.users_action_reset_mfa_confirm(),
      description: m.users_action_reset_mfa_description(),
      title: (name) => m.users_action_reset_mfa_title({ name }),
    },
    removeShares: {
      confirm: m.users_action_remove_shares_confirm(),
      description: m.users_action_remove_shares_description(),
      title: (name) => m.users_action_remove_shares_title({ name }),
    },
  };
}

export function UserActionDialog({
  account,
  action,
  onOpenChange,
}: {
  account: Account | null;
  action: UserAction | null;
  onOpenChange: (open: boolean) => void;
}) {
  const open = account !== null && action !== null;

  return (
    <ResponsiveDialog alert onOpenChange={onOpenChange} open={open}>
      <ResponsiveDialogContent>
        {/* Remount when the account or action changes so an earlier error does not linger. */}
        {open ? (
          <UserActionForm
            account={account}
            action={action}
            key={`${account.id}:${action}`}
            onDone={() => onOpenChange(false)}
          />
        ) : null}
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}

function UserActionForm({
  account,
  action,
  onDone,
}: {
  account: Account;
  action: UserAction;
  onDone: () => void;
}) {
  const setUserState = useSetUserState();
  const deleteUser = useDeleteUser();
  const removeShares = useRemoveUserShares();
  const resetMfa = useResetUserMfa();

  const mutation = {
    delete: deleteUser,
    disable: setUserState,
    removeShares,
    resetMfa,
  }[action];
  const error = mutation.error as Partial<ErrorBody> | null;
  const copy = buildCopy()[action];

  const handleConfirm = () => {
    switch (action) {
      case "delete": {
        deleteUser.mutate(account.id, { onSuccess: onDone });
        break;
      }
      case "disable": {
        setUserState.mutate(
          { id: account.id, state: "disabled" },
          { onSuccess: onDone }
        );
        break;
      }
      case "removeShares": {
        removeShares.mutate(account.id, {
          onSuccess: ({ removed }) => {
            toast.add({
              title: m.users_action_remove_shares_success({ removed }),
              type: "success",
            });
            onDone();
          },
        });
        break;
      }
      case "resetMfa": {
        resetMfa.mutate(account.id, {
          onSuccess: () => {
            toast.add({
              title: m.users_action_reset_mfa_success({
                name: account.username,
              }),
              type: "success",
            });
            onDone();
          },
        });
        break;
      }
      default: {
        break;
      }
    }
  };

  return (
    <>
      <ResponsiveDialogHeader>
        <ResponsiveDialogTitle>
          {copy.title(
            isDeletedAccount(account)
              ? m.users_deleted_account()
              : account.username
          )}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription>
          {copy.description}
        </ResponsiveDialogDescription>
      </ResponsiveDialogHeader>
      {error && (
        <p className="text-destructive text-sm" role="alert">
          {error.code === "last_administrator"
            ? m.users_action_last_admin_error()
            : apiErrorMessage(error, m.users_action_generic_error())}
        </p>
      )}
      <ResponsiveDialogFooter>
        <Button onClick={onDone} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={mutation.isPending}
          onClick={handleConfirm}
          type="button"
          variant="destructive"
        >
          {mutation.isPending ? m.users_action_processing() : copy.confirm}
        </Button>
      </ResponsiveDialogFooter>
    </>
  );
}
