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
      // then, so it is only checked for presence, for the same reason the
      // server parses it against the widest bounds. An account without one
      // (made through a provider) has nothing to type and proves a recent
      // sign-in instead, so the field is not there at all.
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
   * `section`: a block on the account page, new password fields side by side, button on the right.
   * `page`: the card on the forced password change page, fields stacked, full-width button.
   */
  variant: "page" | "section";
}

/**
 * Password change form. On success the server signs out every other session;
 * this form only has to say so.
 *
 * The two server rejections tied to a field, wrong password and breached
 * password, are shown on that field so the user can fix one and resubmit.
 *
 * For an account without a password (signed up through a provider) this sets
 * one. There is no current password to type, so the server instead requires a
 * sign-in within the last 10 minutes and answers `reauthentication_required`
 * otherwise. Once set, the returned account has `has_password: true` and the
 * form turns back into a change form.
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
          // A sign-in-again prompt is drawn under the form, so no toast.
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
