import type { components } from "@tessera/api/schema";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@tessera/ui/components/alert-dialog";
import { Button } from "@tessera/ui/components/button";
import { toast } from "@tessera/ui/components/toast";

import {
  useDeleteUser,
  useRemoveUserShares,
  useSetUserState,
} from "@/hooks/use-admin";
import type { AdminUser } from "@/lib/accounts";
import { isDeletedAccount } from "@/lib/accounts";
import * as m from "@/paraglide/messages";

type Account = AdminUser;
type ErrorBody = components["schemas"]["ErrorBody"];

/** 需要先確認才做的動作。「重新啟用」可以直接按,不在這裡。 */
export type UserAction = "delete" | "disable" | "removeShares";

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
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent>
        {/* 換帳號或換動作時重新掛載,上一次的錯誤訊息才不會留著。 */}
        {open ? (
          <UserActionForm
            account={account}
            action={action}
            key={`${account.id}:${action}`}
            onDone={() => onOpenChange(false)}
          />
        ) : null}
      </AlertDialogContent>
    </AlertDialog>
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

  const mutation = {
    delete: deleteUser,
    disable: setUserState,
    removeShares,
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
      default: {
        break;
      }
    }
  };

  return (
    <>
      <AlertDialogHeader>
        <AlertDialogTitle>
          {copy.title(
            isDeletedAccount(account)
              ? m.users_deleted_account()
              : account.username
          )}
        </AlertDialogTitle>
        <AlertDialogDescription>{copy.description}</AlertDialogDescription>
      </AlertDialogHeader>
      {error && (
        <p className="text-destructive text-sm" role="alert">
          {/* 409 只會是「他是最後一個持有 roles:write 的人」,server 的訊息是英文。 */}
          {error.code === "conflict"
            ? m.users_action_last_admin_error()
            : (error.message ?? m.users_action_generic_error())}
        </p>
      )}
      <AlertDialogFooter>
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
      </AlertDialogFooter>
    </>
  );
}
