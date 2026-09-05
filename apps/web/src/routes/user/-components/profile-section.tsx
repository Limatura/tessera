import { zodResolver } from "@hookform/resolvers/zod";
import { IconDeviceFloppy, IconX } from "@tabler/icons-react";
import { Avatar, AvatarFallback } from "@tessera/ui/components/avatar";
import { Button } from "@tessera/ui/components/button";
import { Field, FieldError, FieldLabel } from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useForm } from "react-hook-form";
import { z } from "zod";

import { useSession } from "@/hooks/use-auth";
import * as m from "@/paraglide/messages";

import { notifyComingSoon } from "./coming-soon";
import { Section } from "./section";

const profileSchema = z.object({
  displayName: z.string().min(1),
  email: z.email({ error: () => m.auth_email_invalid() }),
});

type ProfileValues = z.infer<typeof profileSchema>;

export function ProfileSection() {
  const { data: session } = useSession();
  const defaultValues: ProfileValues = {
    displayName: session?.username ?? "",
    email: session?.email ?? "",
  };

  const {
    formState: { errors, isDirty },
    handleSubmit,
    register,
    reset,
  } = useForm<ProfileValues>({
    defaultValues,
    resolver: zodResolver(profileSchema),
    values: defaultValues,
  });

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

      <form className="space-y-4" onSubmit={handleSubmit(notifyComingSoon)}>
        <div className="grid gap-4 md:grid-cols-2">
          <Field data-invalid={Boolean(errors.displayName)}>
            <FieldLabel htmlFor="profile-display-name">
              {m.user_profile_display_name()}
            </FieldLabel>
            <Input id="profile-display-name" {...register("displayName")} />
            {errors.displayName ? (
              <FieldError errors={[errors.displayName]} />
            ) : null}
          </Field>
          <Field data-invalid={Boolean(errors.email)}>
            <FieldLabel htmlFor="profile-email">
              {m.user_profile_email()}
            </FieldLabel>
            <Input id="profile-email" type="email" {...register("email")} />
            {errors.email ? <FieldError errors={[errors.email]} /> : null}
          </Field>
        </div>

        <div className="flex justify-end gap-2">
          <Button
            disabled={!isDirty}
            onClick={() => reset(defaultValues)}
            type="button"
            variant="ghost"
          >
            <IconX size={16} />
            {m.user_profile_reset()}
          </Button>
          <Button disabled={!isDirty} type="submit" variant="outline">
            <IconDeviceFloppy size={16} />
            {m.user_profile_save()}
          </Button>
        </div>
      </form>
    </Section>
  );
}
