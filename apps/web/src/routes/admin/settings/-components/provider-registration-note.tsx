import { IconInfoCircle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

/**
 * 第三方登入遇到「這個人這裡還沒有帳號」時會怎麼做。
 *
 * 決定權在 `auth.registration` —— 同一個設定同時管密碼註冊跟第三方登入建帳號
 * （見 `api/oidc.rs` 的 `register`）。但它在另一頁，而且那頁的說明只講密碼，
 * 所以在這裡設 provider 的人沒有任何線索知道「關了註冊之後，第三方登入只剩
 * 現有帳號能用」。這一段就是補那條線索。三個 provider 都適用，所以整頁只放一份。
 *
 * 值一律讀 server 上實際生效的設定，不是表單狀態 —— 它講的是這台 instance 現在
 * 的行為（跟 `GuestAnonymousWarning` 同一個理由）。
 */
export function ProviderRegistrationNote() {
  const { data } = useSettings();
  const registration = readSetting(data, "auth.registration");

  return (
    <SettingsNotice
      icon={<IconInfoCircle />}
      title={headline(registration)}
      tone="info"
    >
      {detail(registration)} {m.settings_provider_registration_change_before()}
      <Link
        className="mx-1 underline underline-offset-4 hover:text-foreground"
        to="/admin/settings/account"
      >
        {m.settings_provider_registration_link_label()}
      </Link>
      {m.settings_provider_registration_change_after()}
    </SettingsNotice>
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
