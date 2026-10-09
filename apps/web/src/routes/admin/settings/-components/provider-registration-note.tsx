import { IconAlertOctagon, IconInfoCircle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import type { ProviderRegistrationMode } from "@/lib/admin/settings-schema";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

/**
 * 第三方登入遇到「這個人這裡還沒有帳號」時會怎麼做。
 *
 * 由兩個設定合起來決定：這家的 `allow_registration`，以及它是 `inherit` 時才看的
 * 全域 `auth.registration`（見 server `config.rs` 的 `ProviderRegistration`）。
 * 只講其中一個會講錯 —— `always` 不看全域、也不用邀請碼；`never` 不管全域怎麼設
 * 都只有既有帳號能進。所以每張卡各放一份，而不是整頁一份。
 *
 * `mode` 是表單上的值（這張卡存下去會怎樣），全域設定讀 server 上實際生效的值
 * —— 它在另一頁，這裡改不到（跟 `GuestAnonymousWarning` 同一個理由）。
 *
 * `openToWorld`：provider 自己沒有任何門檻時（GitHub / Discord，或沒填限定群組的
 * OIDC）要講的話。配上 `always`，等於這個 provider 上的任何帳號都能在這台開戶，
 * 所以用最高等級；文案兩邊不同（OIDC 能靠填群組解決，另外兩家不能），由呼叫端給。
 */
export function ProviderRegistrationNote({
  mode,
  openToWorld,
}: {
  mode: ProviderRegistrationMode;
  openToWorld?: { title: string; body: string };
}) {
  const { data } = useSettings();
  const registration = readSetting(data, "auth.registration");

  if (mode === "always" && openToWorld) {
    return (
      <SettingsNotice
        icon={<IconAlertOctagon />}
        title={openToWorld.title}
        tone="danger"
      >
        {openToWorld.body}
      </SettingsNotice>
    );
  }

  if (mode === "always") {
    return (
      <SettingsNotice
        icon={<IconInfoCircle />}
        title={m.settings_provider_registration_headline_always()}
        tone="info"
      >
        {m.settings_provider_registration_detail_always()}
      </SettingsNotice>
    );
  }

  if (mode === "never") {
    return (
      <SettingsNotice
        icon={<IconInfoCircle />}
        title={m.settings_provider_registration_headline_never()}
        tone="info"
      >
        {m.settings_provider_registration_detail_never()}
      </SettingsNotice>
    );
  }

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

/** 「新帳號」下拉選單的選項，OIDC 跟 GitHub / Discord 共用。 */
export function providerRegistrationOptions() {
  return [
    {
      label: m.settings_provider_registration_option_inherit(),
      value: "inherit",
    },
    {
      label: m.settings_provider_registration_option_always(),
      value: "always",
    },
    { label: m.settings_provider_registration_option_never(), value: "never" },
  ];
}

/**
 * 認不得的值當成 `open`：那是 server 的預設，說錯話不如說最可能那句。三個
 * 值以外的東西只會在 server 加了新模式、面板還沒跟上時出現。
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
