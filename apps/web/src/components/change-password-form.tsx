import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy } from "@tabler/icons-react";
import { useChangePassword } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { toast } from "@tessera/ui/components/toast";
import { cn } from "@tessera/ui/lib/utils";
import { useMemo } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";

import { PasswordRequirements } from "@/components/password-requirements";
import { SignInAgain } from "@/components/sign-in-again";
import { useHasPassword } from "@/hooks/use-account";
import { usePasswordBreach } from "@/hooks/use-password-breach";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import { apiErrorCode, apiErrorMessage } from "@/lib/api-error";
import {
  isBreachedPasswordError,
  passwordSchemaFor,
} from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

function changePasswordSchemaFor(policy: PasswordPolicy, hasPassword: boolean) {
  return z
    .object({
      confirmPassword: z.string(),
      // The current password was chosen under whatever policy was in force
      // then, so it is only checked for presence — the same reason the server
      // parses it against the widest bounds. An account without one (made
      // through a provider) has nothing to type and proves a recent sign-in
      // instead, so the field is not there at all.
      currentPassword: hasPassword
        ? z.string().min(1, { error: () => m.auth_password_required() })
        : z.string(),
      newPassword: passwordSchemaFor(policy),
    })
    .refine((values) => values.newPassword === values.confirmPassword, {
      error: () => m.auth_password_mismatch(),
      path: ["confirmPassword"],
    });
}

interface ChangePasswordValues {
  confirmPassword: string;
  currentPassword: string;
  newPassword: string;
}

interface ChangePasswordFormProps {
  onSuccess?: () => void;
  /**
   * `section`：個人設定頁裡的一塊，新密碼兩欄並排、按鈕靠右。
   * `page`：強制改密碼那頁的卡片，欄位直排、按鈕滿版。
   */
  variant: "page" | "section";
}

/**
 * 改密碼表單。成功後 server 會登出其他所有 session，這裡只負責講清楚這件事。
 *
 * 密碼錯、密碼外洩這兩種 server 拒絕標在對應欄位上 —— 改一格再送就好，不用整張重填。
 *
 * 沒有密碼的帳號（只用第三方註冊的）在這裡是「設定密碼」：沒有目前密碼可打，
 * 後端改看 10 分鐘內有沒有登入過，太久就回 `reauthentication_required`（#434）。
 * 設好之後回應的帳號 `has_password` 已是 `true`，表單自己變回「改密碼」。
 */
export function ChangePasswordForm({
  onSuccess,
  variant,
}: ChangePasswordFormProps) {
  const policy = usePasswordPolicy();
  const hasPassword = useHasPassword();
  const schema = useMemo(
    () => changePasswordSchemaFor(policy, hasPassword),
    [policy, hasPassword]
  );
  const changePassword = useChangePassword();
  const needsSignIn =
    apiErrorCode(changePassword.error) === "reauthentication_required";
  const {
    formState: { errors },
    handleSubmit,
    register,
    reset,
    setError,
    watch,
  } = useForm<ChangePasswordValues>({
    defaultValues: {
      confirmPassword: "",
      currentPassword: "",
      newPassword: "",
    },
    resolver: zodResolver(schema),
  });

  const newPassword = watch("newPassword");
  const breached = usePasswordBreach(newPassword, policy);

  const onSubmit = (values: ChangePasswordValues) => {
    // Already shown under the field; the server would only say it again.
    if (breached) {
      return;
    }
    changePassword.mutate(
      {
        current_password: hasPassword ? values.currentPassword : undefined,
        new_password: values.newPassword,
      },
      {
        onError: (error) => {
          // 表單底下會畫重新登入的提示，不用再跳 toast。
          if (apiErrorCode(error) === "reauthentication_required") {
            return;
          }
          if (apiErrorCode(error) === "current_password_incorrect") {
            setError("currentPassword", {
              message: apiErrorMessage(error),
              type: "server",
            });
            return;
          }
          if (isBreachedPasswordError(error)) {
            setError("newPassword", {
              message: m.auth_password_breached(),
              type: "server",
            });
            return;
          }
          toast.add({
            description: apiErrorMessage(error),
            title: m.auth_error(),
            type: "error",
          });
        },
        onSuccess: () => {
          reset();
          toast.add({
            description: hasPassword
              ? m.user_password_changed()
              : m.user_password_set_done(),
            title: m.auth_success(),
            type: "success",
          });
          onSuccess?.();
        },
      }
    );
  };

  const isPage = variant === "page";

  return (
    <form className="space-y-4" onSubmit={handleSubmit(onSubmit)}>
      {hasPassword ? (
        <Field data-invalid={Boolean(errors.currentPassword)}>
          <FieldLabel htmlFor="password-current">
            {m.user_password_current()}
          </FieldLabel>
          <Input
            aria-invalid={Boolean(errors.currentPassword)}
            autoComplete="current-password"
            id="password-current"
            type="password"
            {...register("currentPassword")}
          />
          {errors.currentPassword ? (
            <FieldError errors={[errors.currentPassword]} />
          ) : null}
        </Field>
      ) : null}

      <div className={cn("grid gap-4", !isPage && "md:grid-cols-2")}>
        <Field data-invalid={Boolean(errors.newPassword)}>
          <FieldLabel htmlFor="password-new">
            {m.user_password_new()}
          </FieldLabel>
          <Input
            aria-invalid={Boolean(errors.newPassword)}
            autoComplete="new-password"
            id="password-new"
            type="password"
            {...register("newPassword")}
          />
          <PasswordRequirements
            breached={breached}
            invalid={Boolean(errors.newPassword)}
            policy={policy}
            value={newPassword}
          />
          {errors.newPassword?.type === "server" && !breached ? (
            <FieldError errors={[errors.newPassword]} />
          ) : null}
        </Field>
        <Field data-invalid={Boolean(errors.confirmPassword)}>
          <FieldLabel htmlFor="password-confirm">
            {m.user_password_confirm()}
          </FieldLabel>
          <Input
            aria-invalid={Boolean(errors.confirmPassword)}
            autoComplete="new-password"
            id="password-confirm"
            type="password"
            {...register("confirmPassword")}
          />
          {errors.confirmPassword ? (
            <FieldError errors={[errors.confirmPassword]} />
          ) : null}
        </Field>
      </div>

      {needsSignIn ? <SignInAgain message={m.user_reauth_required()} /> : null}

      <div className={cn("flex", isPage ? "pt-2" : "justify-end")}>
        <Button
          className={cn(isPage && "w-full")}
          disabled={changePassword.isPending}
          type="submit"
          variant={isPage ? "default" : "outline"}
        >
          <IconDeviceFloppy size={16} />
          {submitLabel(changePassword.isPending, hasPassword)}
        </Button>
      </div>
    </form>
  );
}

function submitLabel(isPending: boolean, hasPassword: boolean): string {
  if (isPending) {
    return m.user_password_updating();
  }
  return hasPassword ? m.user_password_update() : m.user_password_set();
}
