import { IconKey, IconPlus } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";

import * as m from "@/paraglide/messages";

import { notifyComingSoon } from "./coming-soon";
import { EmptyBox, Section } from "./section";

export function ApiKeysSection() {
  return (
    <Section
      action={
        <Button
          onClick={notifyComingSoon}
          size="sm"
          type="button"
          variant="outline"
        >
          <IconPlus size={16} />
          {m.user_api_keys_new()}
        </Button>
      }
      description={m.user_api_keys_description()}
      title={m.user_api_keys_title()}
    >
      <EmptyBox icon={<IconKey size={20} />}>
        {m.user_api_keys_empty()}
      </EmptyBox>
    </Section>
  );
}
