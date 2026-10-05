import { IconInfoCircle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import * as m from "@/paraglide/messages";

/**
 * 第三方登入遇到「這個人這裡還沒有帳號」時會怎麼做。
 *
 * 決定權在 `auth.registration` —— 同一個設定同時管密碼註冊跟第三方登入建帳號
 * （見 `api/oidc.rs` 的 `register`）。但它在另一頁，而且那頁的說明只講密碼，
 * 所以在這裡設 provider 的人沒有任何線索知道「關了註冊之後，第三方登入只剩
 * 現有帳號能用」。這一段就是補那條線索。
 *
 * 值一律讀 server 上實際生效的設定，不是表單狀態 —— 它講的是這台 instance 現在
 * 的行為（跟 `GuestAnonymousWarning` 同一個理由）。
 */
export function ProviderRegistrationNote() {
  const { data } = useSettings();
  const registration = readSetting(data, "auth.registration");

  return (
    <div className="flex gap-3 rounded-md border bg-muted/40 p-3 text-sm">
      <IconInfoCircle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <div className="space-y-1">
        <p className="font-medium">{headline(registration)}</p>
        <p className="text-muted-foreground">
          {detail(registration)}
          {m.settings_provider_registration_change_before()}
          <Link
            className="mx-1 underline underline-offset-4"
            to="/admin/settings/account"
          >
            {m.settings_provider_registration_link_label()}
          </Link>
          {m.settings_provider_registration_change_after()}
        </p>
      </div>
    </div>
  );
}

/**
 * 認不得的值當成 `open` —— 那是 server 的預設，說錯話不如說最可能的那句。
 * 三個值以外的東西只會在 server 加了新模式、面板還沒跟上時出現。
 */
function headline(registration: unknown): string {
  switch (registration) {
    case "closed": {
      return m.settings_provider_registration_headline_closed();
    }
    case "invite": {
      return m.settings_provider_registration_headline_invite();
    }
    default: {
      return m.settings_provider_registration_headline_open();
    }
  }
}

function detail(registration: unknown): string {
  switch (registration) {
    case "closed": {
      return m.settings_provider_registration_detail_closed();
    }
    case "invite": {
      return m.settings_provider_registration_detail_invite();
    }
    default: {
      return m.settings_provider_registration_detail_open();
    }
  }
}
