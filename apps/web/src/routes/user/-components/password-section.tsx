import { ChangePasswordForm } from "@/components/change-password-form";
import * as m from "@/paraglide/messages";

import { Section } from "./section";

export function PasswordSection() {
  return (
    <Section
      description={m.user_password_description()}
      title={m.user_password_title()}
    >
      <ChangePasswordForm variant="section" />
    </Section>
  );
}
