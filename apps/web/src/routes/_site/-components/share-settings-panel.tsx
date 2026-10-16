import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import { toast } from "@tessera/ui/components/toast";
import { Flame, KeyRound } from "lucide-react";
import { useState } from "react";

import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import { updateShare } from "@/lib/upload/share-api";
import type { ShareSettings } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";

import { ShareExpiryField } from "./share-expiry-field";
import { SharePasswordField } from "./share-password-field";

/**
 * The three settings after a change, for callers that keep them (the recent uploads list,
 * my shares).
 */
export interface ShareSettingsSnapshot {
  /** Minutes from now; `undefined` never expires. */
  expiresInMinutes?: number;
  burnAfterReading: boolean;
  hasPassword: boolean;
  /**
   * End-to-end encrypted with a key from a password. The share's password is
   * that key's token, so it can be neither changed nor removed.
   */
  passwordIsKey?: boolean;
}

/**
 * The three things that can change after a share is created: expiry,
 * burn-after-reading and password.
 *
 * Shared by the post-upload result screen and the later "manage share"
 * dialog, so the caller decides authorisation: an anonymous upload passes
 * `managementToken`, a signed-in user's own share passes none and uses the
 * session.
 *
 * Expiry and burn-after-reading update optimistically: the screen changes
 * first and reverts with an error on failure. The password needs typing and
 * a save, so `SharePasswordField` handles it.
 */
export function ShareSettingsPanel({
  code,
  managementToken,
  initial,
  onChange,
}: {
  code: string;
  managementToken?: string;
  initial: ShareSettingsSnapshot;
  onChange?: (snapshot: ShareSettingsSnapshot) => void;
}) {
  const capabilities = useCapabilities();
  const [expiresInMinutes, setExpiresInMinutes] = useState(
    initial.expiresInMinutes
  );
  const [burnAfterReading, setBurnAfterReading] = useState(
    initial.burnAfterReading
  );
  const [hasPassword, setHasPassword] = useState(initial.hasPassword);

  const applySettings = async (
    settings: ShareSettings,
    next: ShareSettingsSnapshot,
    rollback: () => void
  ) => {
    try {
      await updateShare(code, settings, managementToken);
      onChange?.(next);
    } catch (error) {
      rollback();
      toast.add({
        description: apiErrorMessage(error, m.common_try_again()),
        title: m.upload_result_update_failed(),
        type: "error",
      });
    }
  };

  const handleExpiryChange = (next: number | undefined) => {
    const previous = expiresInMinutes;
    setExpiresInMinutes(next);
    applySettings(
      { expiresInMinutes: next },
      { burnAfterReading, expiresInMinutes: next, hasPassword },
      () => setExpiresInMinutes(previous)
    );
  };

  // `max_uses` counts retrievals from now, so turning it on after the share was opened
  // still allows one more.
  const handleBurnAfterReadingChange = (checked: boolean) => {
    setBurnAfterReading(checked);
    applySettings(
      { maxUses: checked ? 1 : undefined },
      { burnAfterReading: checked, expiresInMinutes, hasPassword },
      () => setBurnAfterReading(!checked)
    );
  };

  const handlePasswordChange = (next: boolean) => {
    setHasPassword(next);
    onChange?.({ burnAfterReading, expiresInMinutes, hasPassword: next });
  };

  return (
    <div className="flex flex-col gap-3">
      <ShareExpiryField
        maxHours={capabilities.data?.max_expiry_hours}
        minutes={expiresInMinutes}
        onChange={handleExpiryChange}
      />

      <div className="flex items-center justify-between gap-2">
        <Label
          className="text-current"
          htmlFor="share-result-burn-after-reading"
        >
          <Flame className="size-3.5" />
          {m.upload_settings_burn_after_reading()}
        </Label>
        <Switch
          checked={burnAfterReading}
          id="share-result-burn-after-reading"
          onCheckedChange={handleBurnAfterReadingChange}
        />
      </div>

      {initial.passwordIsKey ? (
        <p className="flex items-center gap-2 font-medium text-xs/relaxed">
          <KeyRound className="size-3.5" />
          {m.upload_result_password_is_key()}
        </p>
      ) : (
        <SharePasswordField
          code={code}
          initialHasPassword={initial.hasPassword}
          managementToken={managementToken}
          onChange={handlePasswordChange}
        />
      )}
    </div>
  );
}

/**
 * Stores "minutes from now" as an absolute time: by the time the list is opened the next
 * day, a relative time is wrong.
 */
export function expiresAtFrom(minutes: number | undefined): string | null {
  if (minutes === undefined || minutes === 0) {
    return null;
  }
  return new Date(Date.now() + minutes * 60 * 1000).toISOString();
}
