import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy, IconX } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { useSession } from "@tessera/auth/hooks";
import { Avatar, AvatarFallback } from "@tessera/ui/components/avatar";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { toast } from "@tessera/ui/components/toast";
import { useMemo } from "react";
import type * as React from "react";
import { useForm } from "react-hook-form";
import type { FieldError as FormFieldError } from "react-hook-form";
import { z } from "zod";

import { SignInAgain } from "@/components/sign-in-again";
import { useHasPassword, useUpdateAccount } from "@/hooks/use-account";
import { apiErrorCode, apiErrorMessage } from "@/lib/api-error";
import { usernameSchema } from "@/lib/api/auth-schema";
import * as m from "@/paraglide/messages";

import { EmailVerificationStatus } from "./email-verification-status";
import { Section } from "./section";

type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * An account without a password (provider sign-up only) has nothing to type and relies
 * on a recent sign-in instead, so the field is required only when there is a password.
 */
function profileSchemaFor(hasPassword: boolean) {
  return z.object({
    // Only check it is filled in: it was set long ago and need not meet the
    // current policy (as in PasswordSection).
    currentPassword: hasPassword
      ? z.string().min(1, { error: () => m.auth_password_required() })
      : z.string(),
    email: z.email({ error: () => m.auth_email_invalid() }),
    username: usernameSchema,
  });
}

interface ProfileValues {
  currentPassword: string;
  email: string;
  username: string;
}

export function ProfileSection() {
  const { data: session } = useSession();
  const updateAccount = useUpdateAccount();
  const hasPassword = useHasPassword();
  const schema = useMemo(() => profileSchemaFor(hasPassword), [hasPassword]);
  const defaultValues: ProfileValues = {
    currentPassword: "",
    email: session?.email ?? "",
    username: session?.username ?? "",
  };

  const {
    formState: { dirtyFields, errors },
    handleSubmit,
    register,
    reset,
    setError,
  } = useForm<ProfileValues>({
    defaultValues,
    resolver: zodResolver(schema),
    values: defaultValues,
  });

  // Typing only the password is not a change: with no name or email to send, nothing is saved.
  const changed = Boolean(dirtyFields.username || dirtyFields.email);
  const needsSignIn =
    apiErrorCode(updateAccount.error) === "reauthentication_required";

  const handleSave = (values: ProfileValues) => {
    const emailChanged = values.email !== session?.email;
    updateAccount.mutate(
      {
        current_password: hasPassword ? values.currentPassword : undefined,
        // Unchanged fields are left out: the server treats omission as "keep",
        // and resending a value costs another uniqueness check.
        email: emailChanged ? values.email : undefined,
        username:
          values.username === session?.username ? undefined : values.username,
      },
      {
        // openapi-fetch throws the ErrorBody; a dropped connection is a TypeError with no code.
        onError: (error) => {
          const { code } = error as Partial<ErrorBody>;
          // The form already shows a sign-in-again prompt; no toast needed.
          if (code === "reauthentication_required") {
            return;
          }
          if (code === "current_password_incorrect") {
            setError("currentPassword", {
              message: m.user_profile_wrong_password(),
            });
            return;
          }
          toast.add({
            description:
              code === "username_taken" || code === "email_taken"
                ? m.user_profile_taken()
                : apiErrorMessage(error, m.common_try_again()),
            title: m.user_profile_save_failed(),
            type: "error",
          });
        },
        onSuccess: () => {
          toast.add({
            description: emailChanged
              ? m.user_profile_email_unverified()
              : undefined,
            title: m.user_profile_saved(),
            type: "success",
          });
        },
      }
    );
  };

  return (
    <Section
      description={m.user_profile_description()}
      title={m.user_profile_title()}
    >
      <div className="flex items-center gap-3">
        <Avatar size="lg">
          <AvatarFallback>
            {(session?.username ?? "?").charAt(0).toUpperCase()}
          </AvatarFallback>
        </Avatar>
        <div>
          <p className="font-medium text-sm">@{session?.username ?? ""}</p>
          <p className="text-muted-foreground text-xs">
            {m.user_profile_local_account()}
          </p>
        </div>
      </div>

      <form className="space-y-4" onSubmit={handleSubmit(handleSave)}>
        <div className="grid gap-4 md:grid-cols-2">
          <Field data-invalid={Boolean(errors.username)}>
            <div className="flex h-8 items-center">
              <FieldLabel htmlFor="profile-username">
                {m.user_profile_display_name()}
              </FieldLabel>
            </div>
            <Input
              autoComplete="username"
              id="profile-username"
              {...register("username")}
            />
            {errors.username ? <FieldError errors={[errors.username]} /> : null}
          </Field>
          <Field data-invalid={Boolean(errors.email)}>
            {/*
             * The status describes the address in the session. Hidden mid-edit, so
             * "verified" does not seem to describe the new address being typed.
             * `undefined` means an older server without the field, so nothing is guessed.
             * The row has a fixed height so the status never pushes the input down.
             */}
            <div className="flex h-8 items-center justify-between gap-2">
              <FieldLabel htmlFor="profile-email">
                {m.user_profile_email()}
              </FieldLabel>
              {!dirtyFields.email &&
              session?.email_verified_at !== undefined ? (
                <EmailVerificationStatus
                  verifiedAt={session.email_verified_at}
                />
              ) : null}
            </div>
            <Input
              autoComplete="email"
              id="profile-email"
              type="email"
              {...register("email")}
            />
            {errors.email ? <FieldError errors={[errors.email]} /> : null}
          </Field>
        </div>

        {changed && hasPassword ? (
          <CurrentPasswordField
            error={errors.currentPassword}
            {...register("currentPassword")}
          />
        ) : null}

        {needsSignIn ? (
          <SignInAgain message={m.user_reauth_required()} />
        ) : null}

        <div className="flex justify-end gap-2">
          <Button
            disabled={!changed || updateAccount.isPending}
            onClick={() => reset(defaultValues)}
            type="button"
            variant="ghost"
          >
            <IconX size={16} />
            {m.user_profile_reset()}
          </Button>
          <Button
            disabled={!changed || updateAccount.isPending}
            type="submit"
            variant="outline"
          >
            <IconDeviceFloppy size={16} />
            {m.user_profile_save()}
          </Button>
        </div>
      </form>
    </Section>
  );
}

/**
 * Shown when the name or email changed and the account has a password. Accounts
 * without one rely on a recent sign-in.
 */
function CurrentPasswordField({
  error,
  ...input
}: { error: FormFieldError | undefined } & React.ComponentProps<typeof Input>) {
  return (
    <Field data-invalid={Boolean(error)}>
      <FieldLabel htmlFor="profile-current-password">
        {m.user_password_current()}
      </FieldLabel>
      <Input
        autoComplete="current-password"
        id="profile-current-password"
        type="password"
        {...input}
      />
      {error ? (
        <FieldError errors={[error]} />
      ) : (
        <FieldDescription>
          {m.user_profile_current_password_hint()}
        </FieldDescription>
      )}
    </Field>
  );
}
