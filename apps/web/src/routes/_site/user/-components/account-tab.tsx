import { Separator } from "@tessera/ui/components/separator";

import { ApiKeysSection } from "./api-keys-section";
import { DangerZone } from "./danger-zone";
import { IdentitiesSection } from "./identities-section";
import { MfaSection } from "./mfa-section";
import { PasskeysSection } from "./passkeys-section";
import { PasswordSection } from "./password-section";
import { ProfileSection } from "./profile-section";
import { SessionsSection } from "./sessions-section";
import { WebhooksSection } from "./webhooks-section";

export function AccountTab() {
  return (
    <div className="space-y-8">
      <ProfileSection />
      <Separator />
      <ApiKeysSection />
      <Separator />
      <WebhooksSection />
      <PasswordSection />
      <Separator />
      <MfaSection />
      <Separator />
      <PasskeysSection />
      <Separator />
      <IdentitiesSection />
      <Separator />
      <SessionsSection />
      <Separator />
      <DangerZone />
    </div>
  );
}
