import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useMemo } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";

import { PasswordRequirements } from "@/components/password-requirements";
import { usePasswordPolicy } from "@/hooks/use-password-policy";
import { passwordSchemaFor } from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";
import * as m from "@/paraglide/messages";

import { notifyComingSoon } from "./coming-soon";
import { Section } from "./section";

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

export function PasswordSection() {
  const policy = usePasswordPolicy();
  const schema = useMemo(() => changePasswordSchemaFor(policy), [policy]);
  const {
    formState: { errors },
    handleSubmit,
    register,
    reset,
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

  // API 還沒接:驗證是真的,送出只提示尚未開放,並清掉打過的密碼。
  const onSubmit = () => {
    reset();
    notifyComingSoon();
  };

  return (
    <Section
      description={m.user_password_description()}
      title={m.user_password_title()}
    >
      <form className="space-y-4" onSubmit={handleSubmit(onSubmit)}>
        <Field data-invalid={Boolean(errors.currentPassword)}>
          <FieldLabel htmlFor="password-current">
            {m.user_password_current()}
          </FieldLabel>
          <Input
            autoComplete="current-password"
            id="password-current"
            type="password"
            {...register("currentPassword")}
          />
          {errors.currentPassword ? (
            <FieldError errors={[errors.currentPassword]} />
          ) : null}
        </Field>

        <div className="grid gap-4 md:grid-cols-2">
          <Field data-invalid={Boolean(errors.newPassword)}>
            <FieldLabel htmlFor="password-new">
              {m.user_password_new()}
            </FieldLabel>
            <Input
              autoComplete="new-password"
              id="password-new"
              type="password"
              {...register("newPassword")}
            />
            <PasswordRequirements
              invalid={Boolean(errors.newPassword)}
              policy={policy}
              value={newPassword}
            />
          </Field>
          <Field data-invalid={Boolean(errors.confirmPassword)}>
            <FieldLabel htmlFor="password-confirm">
              {m.user_password_confirm()}
            </FieldLabel>
            <Input
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

        <div className="flex justify-end">
          <Button type="submit" variant="outline">
            <IconDeviceFloppy size={16} />
            {m.user_password_update()}
          </Button>
        </div>
      </form>
    </Section>
  );
}
