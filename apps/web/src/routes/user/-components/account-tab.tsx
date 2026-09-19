import { Separator } from "@tessera/ui/components/separator";

import { ApiKeysSection } from "./api-keys-section";
import { DangerZone } from "./danger-zone";
import { PasskeysSection } from "./passkeys-section";
import { PasswordSection } from "./password-section";
import { ProfileSection } from "./profile-section";
import { SessionsSection } from "./sessions-section";

export function AccountTab() {
  return (
    <div className="space-y-8">
      <ProfileSection />
      <Separator />
      <ApiKeysSection />
      <Separator />
      <PasswordSection />
      <Separator />
      <PasskeysSection />
      <Separator />
      <SessionsSection />
      <Separator />
      <DangerZone />
    </div>
  );
}
