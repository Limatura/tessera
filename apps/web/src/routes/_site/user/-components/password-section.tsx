import { ChangePasswordForm } from "@/components/change-password-form";
import { useHasPassword } from "@/hooks/use-account";
import * as m from "@/paraglide/messages";

import { Section } from "./section";

/**
 * Accounts without a password (signed up through a provider) see "Set password" here;
 * see ChangePasswordForm.
 */
export function PasswordSection() {
  const hasPassword = useHasPassword();

  return (
    <Section
      description={
        hasPassword
          ? m.user_password_description()
          : m.user_password_set_description()
      }
      title={
        hasPassword ? m.user_password_title() : m.user_password_set_title()
      }
    >
      <ChangePasswordForm variant="section" />
    </Section>
  );
}
