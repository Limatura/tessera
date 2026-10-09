import { IconAlertTriangle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

const GUEST_ROLE = "guest";

/**
 * 兩個設定各在一頁，`fixAt` 指「另外那一頁」——這一頁修不到、要跳過去改的那
 * 一個：帳號與註冊那頁改預設角色，上傳設置那頁關匿名上傳。
 *
 * 是函式而不是 module-level const：裡面的 `m.xxx()` 要在渲染當下讀 locale，
 * 包成 const 只會在模組第一次載入時凍住語系。
 */
function fixTargets() {
  return {
    account: {
      action: m.settings_guest_warning_account_action(),
      label: m.settings_guest_warning_account_label(),
      to: "/admin/settings/account",
    },
    upload: {
      action: m.settings_guest_warning_upload_action(),
      label: m.settings_guest_warning_upload_label(),
      to: "/admin/settings/upload",
    },
  } as const;
}

/**
 * `auth.default_role` 選了 `guest`、`features.anonymous_upload` 又開著時的提醒。
 *
 * 分屬兩個頁面，兩邊都掛一份。值一律讀 server 上實際生效的設定而非表單狀態
 * ——講的是這台 instance 現在的樣子；正在改 `default_role` 的那頁可用
 * `defaultRole` 把還沒存的值蓋進來先預覽。
 *
 * 為什麼要提醒：guest 雖是空權限，但限制的是「帳號」不是「instance」。匿名
 * 上傳開著時，同一個人登出就能以陌生人身分上傳，這角色實際限制不到任何人。
 */
export function GuestAnonymousWarning({
  defaultRole,
  fixAt,
}: {
  /** 這一頁表單裡還沒存的 `auth.default_role`，沒給就讀 server 上的。 */
  defaultRole?: string;
  /** 要去哪一頁修 —— 指向另外那個設定所在的頁面。 */
  fixAt: keyof ReturnType<typeof fixTargets>;
}) {
  const { data } = useSettings();

  const role = defaultRole ?? readSetting(data, "auth.default_role");
  const anonymousUpload = readSetting(data, "features.anonymous_upload");

  if (role !== GUEST_ROLE || anonymousUpload !== true) {
    return null;
  }

  const target = fixTargets()[fixAt];

  return (
    <SettingsNotice
      icon={<IconAlertTriangle />}
      title={m.settings_guest_warning_title()}
      tone="warning"
    >
      {m.settings_guest_warning_body_before()}
      <Link
        className="mx-1 underline underline-offset-4 hover:text-foreground"
        to={target.to}
      >
        {target.label}
      </Link>
      {m.settings_guest_warning_body_after({ action: target.action })}
    </SettingsNotice>
  );
}
