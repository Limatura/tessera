import { ChangePasswordForm } from "@/components/change-password-form";
import { useHasPassword } from "@/hooks/use-account";
import * as m from "@/paraglide/messages";

import { Section } from "./section";

/** 沒有密碼的帳號（只用第三方註冊的）在這裡是「設定密碼」，見 ChangePasswordForm。 */
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
