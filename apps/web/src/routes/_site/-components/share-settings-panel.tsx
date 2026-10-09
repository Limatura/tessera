import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import { toast } from "@tessera/ui/components/toast";
import { Flame } from "lucide-react";
import { useState } from "react";

import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import { updateShare } from "@/lib/upload/share-api";
import type { ShareSettings } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";

import { ShareExpiryField } from "./share-expiry-field";
import { SharePasswordField } from "./share-password-field";

/** 這三項改了之後的樣子,給要記住它的呼叫端(最近上傳清單、我的分享)用。 */
export interface ShareSettingsSnapshot {
  /** 從現在起幾分鐘;`undefined` 是永不過期 */
  expiresInMinutes?: number;
  burnAfterReading: boolean;
  hasPassword: boolean;
}

/**
 * 分享建立之後還能改的三項:期限、下載即焚、密碼。
 *
 * 上傳完的結果畫面跟事後的「管理分享」對話框共用這一塊(#364),所以授權方式由呼叫端
 * 決定:匿名上傳帶 `managementToken`,登入者自己的分享不帶、走 session。
 *
 * 期限跟下載即焚是樂觀更新:先改畫面,失敗再退回並跳錯誤。密碼要打字、按儲存,
 * 由 `SharePasswordField` 自己處理。
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

  // `max_uses` 是「從現在起還能取幾次」,分享被開過之後才打開也是再一次。
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

      <SharePasswordField
        code={code}
        initialHasPassword={initial.hasPassword}
        managementToken={managementToken}
        onChange={handlePasswordChange}
      />
    </div>
  );
}

/** 「從現在起幾分鐘」換成絕對時間存起來 —— 清單隔天打開時,相對時間早就不準了。 */
export function expiresAtFrom(minutes: number | undefined): string | null {
  if (minutes === undefined || minutes === 0) {
    return null;
  }
  return new Date(Date.now() + minutes * 60 * 1000).toISOString();
}
