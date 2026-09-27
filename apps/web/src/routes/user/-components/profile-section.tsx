import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy, IconX } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
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
import { useForm } from "react-hook-form";
import { z } from "zod";

import { useUpdateAccount } from "@/hooks/use-account";
import { useSession } from "@/hooks/use-auth";
import { usernameSchema } from "@/lib/api/auth-schema";
import * as m from "@/paraglide/messages";

import { EmailVerificationStatus } from "./email-verification-status";
import { Section } from "./section";

type ErrorBody = components["schemas"]["ErrorBody"];

const profileSchema = z.object({
  // 只檢查有沒有填:這是當初設的密碼,不一定符合現在的政策(同 PasswordSection)。
  currentPassword: z.string().min(1, {
    error: () => m.auth_password_required(),
  }),
  email: z.email({ error: () => m.auth_email_invalid() }),
  username: usernameSchema,
});

type ProfileValues = z.infer<typeof profileSchema>;

export function ProfileSection() {
  const { data: session } = useSession();
  const updateAccount = useUpdateAccount();
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
    resolver: zodResolver(profileSchema),
    values: defaultValues,
  });

  // 只打了密碼不算有改東西 —— 沒有名稱或 email 要送,就沒有東西要存。
  const changed = Boolean(dirtyFields.username || dirtyFields.email);

  const handleSave = (values: ProfileValues) => {
    const emailChanged = values.email !== session?.email;
    updateAccount.mutate(
      {
        current_password: values.currentPassword,
        // 沒改的欄位不送:server 把省略當「保留」,送同一個值會多一次唯一性檢查。
        email: emailChanged ? values.email : undefined,
        username:
          values.username === session?.username ? undefined : values.username,
      },
      {
        // openapi-fetch 丟的是 ErrorBody;斷線時是 TypeError,沒有 code。
        onError: (error) => {
          const { code, message } = error as Partial<ErrorBody>;
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
                : (message ?? m.common_try_again()),
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
            <FieldLabel htmlFor="profile-username">
              {m.user_profile_display_name()}
            </FieldLabel>
            <Input
              autoComplete="username"
              id="profile-username"
              {...register("username")}
            />
            {errors.username ? <FieldError errors={[errors.username]} /> : null}
          </Field>
          <Field data-invalid={Boolean(errors.email)}>
            <FieldLabel htmlFor="profile-email">
              {m.user_profile_email()}
            </FieldLabel>
            <Input
              autoComplete="email"
              id="profile-email"
              type="email"
              {...register("email")}
            />
            {errors.email ? <FieldError errors={[errors.email]} /> : null}
            {/*
             * 狀態講的是 session 裡那個地址。改到一半就不顯示,免得「已驗證」看起來
             * 像是在說剛打的新地址。`undefined` 是舊版 server,沒這個欄位就不猜。
             */}
            {!dirtyFields.email && session?.email_verified_at !== undefined ? (
              <EmailVerificationStatus verifiedAt={session.email_verified_at} />
            ) : null}
          </Field>
        </div>

        {changed ? (
          <Field data-invalid={Boolean(errors.currentPassword)}>
            <FieldLabel htmlFor="profile-current-password">
              {m.user_password_current()}
            </FieldLabel>
            <Input
              autoComplete="current-password"
              id="profile-current-password"
              type="password"
              {...register("currentPassword")}
            />
            {errors.currentPassword ? (
              <FieldError errors={[errors.currentPassword]} />
            ) : (
              <FieldDescription>
                {m.user_profile_current_password_hint()}
              </FieldDescription>
            )}
          </Field>
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
