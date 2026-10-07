import { IconTrash } from "@tabler/icons-react";
import { useNavigate } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { hasPermission, PERMISSIONS } from "@tessera/auth/permissions";
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

import { SignInAgain } from "@/components/sign-in-again";
import { useDeleteAccount, useHasPassword } from "@/hooks/use-account";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

function errorMessage(error: Partial<ErrorBody> | null): string | null {
  if (!error) {
    return null;
  }
  switch (error.code) {
    case "current_password_incorrect": {
      return m.user_profile_wrong_password();
    }
    // 由 SignInAgain 講,這裡不重複。
    case "reauthentication_required": {
      return null;
    }
    // 最後一個持有 `roles:write` 的人刪掉自己,這台就再也沒人能發權限了。
    case "last_administrator": {
      return m.user_danger_delete_last_admin();
    }
    default: {
      return apiErrorMessage(error, m.common_try_again());
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
          {/* 關掉就卸載,下次打開確認文字、密碼跟勾選都回到初始值。 */}
          {open ? <DeleteAccountForm onCancel={() => setOpen(false)} /> : null}
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

function DeleteAccountForm({ onCancel }: { onCancel: () => void }) {
  const navigate = useNavigate();
  const deleteAccount = useDeleteAccount();
  const { data: session } = useSession();
  // 前端不知道自己是不是「最後一位」管理員，只能先提醒；真的是的話後端會回
  // `last_administrator`（#365）。
  const isAdministrator = hasPermission(session, PERMISSIONS.rolesWrite);
  const hasPassword = useHasPassword();
  /*
   * 手打一段確認文字才能按刪除(#436)。這是擋手滑,不是身分確認 —— 那是密碼或
   * 「最近登入過」的事,由後端判斷。允許貼上:擋的是誤按,不是故意要刪的人。
   */
  const phrase = m.user_danger_delete_phrase();
  const [confirmText, setConfirmText] = useState("");
  const confirmed = confirmText.trim() === phrase;
  const [password, setPassword] = useState("");
  // 預設一併刪除:「刪掉我的帳號」多半也包括我傳出去的東西(見 DeleteAccountRequest)。
  const [deleteShares, setDeleteShares] = useState(true);

  const error = errorMessage(deleteAccount.error as Partial<ErrorBody> | null);
  const needsSignIn =
    (deleteAccount.error as Partial<ErrorBody> | null)?.code ===
    "reauthentication_required";
  const canSubmit =
    confirmed &&
    (!hasPassword || password.length > 0) &&
    !deleteAccount.isPending;

  const handleDelete = (event: React.FormEvent) => {
    event.preventDefault();
    deleteAccount.mutate(
      {
        current_password: hasPassword ? password : undefined,
        delete_shares: deleteShares,
      },
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

      {isAdministrator ? (
        <p className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm">
          {m.user_danger_delete_admin_notice()}
        </p>
      ) : null}

      <div className="space-y-4">
        <Field data-invalid={!hasPassword && Boolean(error)}>
          <FieldLabel htmlFor="delete-account-confirm">
            {m.user_danger_delete_type_phrase({ phrase })}
          </FieldLabel>
          <Input
            autoComplete="off"
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- 對話框第一個輸入欄
            autoFocus
            id="delete-account-confirm"
            onChange={(event) => setConfirmText(event.target.value)}
            placeholder={phrase}
            spellCheck={false}
            value={confirmText}
          />
          {!hasPassword && error ? <FieldError>{error}</FieldError> : null}
        </Field>

        {hasPassword ? (
          <Field data-invalid={Boolean(error)}>
            <FieldLabel htmlFor="delete-account-password">
              {m.user_password_current()}
            </FieldLabel>
            <Input
              autoComplete="current-password"
              id="delete-account-password"
              onChange={(event) => setPassword(event.target.value)}
              type="password"
              value={password}
            />
            {error ? <FieldError>{error}</FieldError> : null}
          </Field>
        ) : null}

        {needsSignIn ? (
          <SignInAgain message={m.user_reauth_required()} />
        ) : null}

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
        <Button disabled={!canSubmit} type="submit" variant="destructive">
          {m.user_danger_delete_action()}
        </Button>
      </AlertDialogFooter>
    </form>
  );
}
