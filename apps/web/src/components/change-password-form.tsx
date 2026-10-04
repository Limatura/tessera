import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { toast } from "@tessera/ui/components/toast";
import { cn } from "@tessera/ui/lib/utils";
import { useMemo } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";

import { PasswordRequirements } from "@/components/password-requirements";
import { useChangePassword } from "@/hooks/use-auth";
import { usePasswordBreach } from "@/hooks/use-password-breach";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import { apiErrorCode, apiErrorMessage } from "@/lib/api-error";
import {
  isBreachedPasswordError,
  passwordSchemaFor,
} from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

function changePasswordSchemaFor(policy: PasswordPolicy) {
  return z
    .object({
      confirmPassword: z.string(),
      // The current password was chosen under whatever policy was in force
      // then, so it is only checked for presence — the same reason the server
      // parses it against the widest bounds.
      currentPassword: z.string().min(1, {
        error: () => m.auth_password_required(),
      }),
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
 */
export function ChangePasswordForm({
  onSuccess,
  variant,
}: ChangePasswordFormProps) {
  const policy = usePasswordPolicy();
  const schema = useMemo(() => changePasswordSchemaFor(policy), [policy]);
  const changePassword = useChangePassword();
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
        current_password: values.currentPassword,
        new_password: values.newPassword,
      },
      {
        onError: (error) => {
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
            description: m.user_password_changed(),
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

      <div className={cn("flex", isPage ? "pt-2" : "justify-end")}>
        <Button
          className={cn(isPage && "w-full")}
          disabled={changePassword.isPending}
          type="submit"
          variant={isPage ? "default" : "outline"}
        >
          <IconDeviceFloppy size={16} />
          {changePassword.isPending
            ? m.user_password_updating()
            : m.user_password_update()}
        </Button>
      </div>
    </form>
  );
}
