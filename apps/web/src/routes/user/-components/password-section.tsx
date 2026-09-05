import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useForm } from "react-hook-form";
import { z } from "zod";

import { PASSWORD_MIN_LENGTH, passwordSchema } from "@/lib/api/auth-schema";
import * as m from "@/paraglide/messages";

import { notifyComingSoon } from "./coming-soon";
import { Section } from "./section";

const changePasswordSchema = z
  .object({
    confirmPassword: z.string(),
    currentPassword: z.string().min(1, {
      error: () => m.auth_password_required(),
    }),
    newPassword: passwordSchema,
  })
  .refine((values) => values.newPassword === values.confirmPassword, {
    error: () => m.auth_password_mismatch(),
    path: ["confirmPassword"],
  });

type ChangePasswordValues = z.infer<typeof changePasswordSchema>;

export function PasswordSection() {
  const {
    formState: { errors },
    handleSubmit,
    register,
    reset,
  } = useForm<ChangePasswordValues>({
    defaultValues: {
      confirmPassword: "",
      currentPassword: "",
      newPassword: "",
    },
    resolver: zodResolver(changePasswordSchema),
  });

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
            {errors.newPassword ? (
              <FieldError errors={[errors.newPassword]} />
            ) : (
              <FieldDescription>
                {m.user_password_hint({ min: PASSWORD_MIN_LENGTH })}
              </FieldDescription>
            )}
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
