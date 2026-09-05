import { IconPlus, IconShieldLock } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import * as m from "@/paraglide/messages";

import { notifyComingSoon } from "./coming-soon";
import { EmptyBox, Section } from "./section";

export function PasskeysSection() {
  return (
    <Section
      action={
        <div className="flex gap-2">
          <Button
            onClick={notifyComingSoon}
            size="sm"
            type="button"
            variant="outline"
          >
            <IconShieldLock size={16} />
            {m.user_passkeys_mfa()}
          </Button>
          <Button
            onClick={notifyComingSoon}
            size="sm"
            type="button"
            variant="outline"
          >
            <IconPlus size={16} />
            {m.user_passkeys_add()}
          </Button>
        </div>
      }
      description={m.user_passkeys_description()}
      title={m.user_passkeys_title()}
    >
      <EmptyBox icon={<IconShieldLock size={20} />}>
        {m.user_passkeys_empty()}
      </EmptyBox>
    </Section>
  );
}
