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
import { isDeletedAccount } from "@/lib/accounts";

type Account = components["schemas"]["AccountResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/** 需要先確認才做的動作。「重新啟用」可以直接按,不在這裡。 */
export type UserAction = "delete" | "disable" | "removeShares";

const COPY: Record<
  UserAction,
  { title: (name: string) => string; description: string; confirm: string }
> = {
  delete: {
    confirm: "刪除帳號",
    description:
      "帳號會被清成一列「已刪除的帳號」,無法復原。他上傳的分享不會一起刪掉,會變成匿名分享繼續有效 —— 要清掉請另外用「結束全部分享」。",
    title: (name) => `刪除「${name}」？`,
  },
  disable: {
    confirm: "停用",
    description:
      "他會立刻被登出,之後也登不進來,API token 同時失效。他的分享照樣給收件人。隨時可以重新啟用。",
    title: (name) => `停用「${name}」？`,
  },
  removeShares: {
    confirm: "全部結束",
    description:
      "這個帳號上傳的每一份分享都會立刻失效,已發出去的連結一起打不開,無法復原。帳號本身不受影響。",
    title: (name) => `結束「${name}」的全部分享？`,
  },
};

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
  const copy = COPY[action];

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
            toast.add({ title: `已結束 ${removed} 份分享`, type: "success" });
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
            isDeletedAccount(account) ? "已刪除的帳號" : account.username
          )}
        </AlertDialogTitle>
        <AlertDialogDescription>{copy.description}</AlertDialogDescription>
      </AlertDialogHeader>
      {error && (
        <p className="text-destructive text-sm" role="alert">
          {/* 409 只會是「他是最後一個持有 roles:write 的人」,server 的訊息是英文。 */}
          {error.code === "conflict"
            ? "這是最後一個能發權限的帳號,先把權限交給別人再做。"
            : (error.message ?? "操作失敗,請再試一次")}
        </p>
      )}
      <AlertDialogFooter>
        <Button onClick={onDone} type="button" variant="outline">
          取消
        </Button>
        <Button
          disabled={mutation.isPending}
          onClick={handleConfirm}
          type="button"
          variant="destructive"
        >
          {mutation.isPending ? "處理中…" : copy.confirm}
        </Button>
      </AlertDialogFooter>
    </>
  );
}
