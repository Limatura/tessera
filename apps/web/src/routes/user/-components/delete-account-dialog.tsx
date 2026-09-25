import { IconTrash } from "@tabler/icons-react";
import { useNavigate } from "@tanstack/react-router";
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
import { Checkbox } from "@tessera/ui/components/checkbox";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";

import { useDeleteAccount } from "@/hooks/use-account";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

function errorMessage(error: Partial<ErrorBody> | null): string | null {
  if (!error) {
    return null;
  }
  switch (error.code) {
    case "unauthorized": {
      return m.user_profile_wrong_password();
    }
    // 最後一個持有 `roles:write` 的人刪掉自己,這台就再也沒人能發權限了。
    case "conflict": {
      return m.user_danger_delete_last_admin();
    }
    default: {
      return error.message ?? m.common_try_again();
    }
  }
}

export function DeleteAccountButton() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <Button
        onClick={() => setOpen(true)}
        size="sm"
        type="button"
        variant="destructive"
      >
        <IconTrash size={16} />
        {m.user_danger_delete_action()}
      </Button>
      <AlertDialog onOpenChange={setOpen} open={open}>
        <AlertDialogContent>
          {/* 關掉就卸載,下次打開密碼跟勾選都回到初始值。 */}
          {open ? <DeleteAccountForm onCancel={() => setOpen(false)} /> : null}
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function DeleteAccountForm({ onCancel }: { onCancel: () => void }) {
  const navigate = useNavigate();
  const deleteAccount = useDeleteAccount();
  const [password, setPassword] = useState("");
  // 預設一併刪除:「刪掉我的帳號」多半也包括我傳出去的東西(見 DeleteAccountRequest)。
  const [deleteShares, setDeleteShares] = useState(true);

  const error = errorMessage(deleteAccount.error as Partial<ErrorBody> | null);

  const handleDelete = (event: React.FormEvent) => {
    event.preventDefault();
    deleteAccount.mutate(
      { current_password: password, delete_shares: deleteShares },
      {
        onSuccess: () => {
          toast.add({ title: m.user_danger_delete_done(), type: "success" });
          navigate({ to: "/" });
        },
      }
    );
  };

  return (
    <form className="contents" onSubmit={handleDelete}>
      <AlertDialogHeader>
        <AlertDialogTitle>
          {m.user_danger_delete_confirm_title()}
        </AlertDialogTitle>
        <AlertDialogDescription>
          {m.user_danger_delete_confirm_description()}
        </AlertDialogDescription>
      </AlertDialogHeader>

      <div className="space-y-4">
        <Field data-invalid={Boolean(error)}>
          <FieldLabel htmlFor="delete-account-password">
            {m.user_password_current()}
          </FieldLabel>
          <Input
            autoComplete="current-password"
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- 對話框唯一的輸入欄
            autoFocus
            id="delete-account-password"
            onChange={(event) => setPassword(event.target.value)}
            type="password"
            value={password}
          />
          {error ? <FieldError>{error}</FieldError> : null}
        </Field>

        <Field orientation="horizontal">
          <Checkbox
            checked={deleteShares}
            id="delete-account-shares"
            onCheckedChange={(next) => setDeleteShares(next === true)}
          />
          <FieldContent>
            <FieldLabel htmlFor="delete-account-shares">
              {m.user_danger_delete_shares()}
            </FieldLabel>
            <FieldDescription>
              {deleteShares
                ? m.user_danger_delete_shares_on()
                : m.user_danger_delete_shares_off()}
            </FieldDescription>
          </FieldContent>
        </Field>
      </div>

      <AlertDialogFooter>
        <Button onClick={onCancel} type="button" variant="outline">
          {m.common_cancel()}
        </Button>
        <Button
          disabled={password.length === 0 || deleteAccount.isPending}
          type="submit"
          variant="destructive"
        >
          {m.user_danger_delete_action()}
        </Button>
      </AlertDialogFooter>
    </form>
  );
}
