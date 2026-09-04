import { IconAlertTriangle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/use-settings";

const GUEST_ROLE = "guest";

/**
 * 兩個設定各在一頁，`fixAt` 指的是「另外那一頁」—— 也就是這一頁修不到、要跳過去改的
 * 那一個。動作跟著頁面走：帳號與註冊那頁改的是預設角色，上傳設置那頁關的是匿名上傳。
 */
const FIX_TARGETS = {
  account: {
    action: "改掉新帳號的預設角色",
    label: "帳號與註冊",
    to: "/settings/account",
  },
  upload: {
    action: "關閉「允許訪客上傳」",
    label: "上傳設置",
    to: "/settings/upload",
  },
} as const;

/**
 * `auth.default_role` 選了 `guest`、`features.anonymous_upload` 又開著時的提醒。
 *
 * 這兩個設定分屬兩個頁面，兩邊都掛一份。值一律讀 server 上實際生效的設定而不是表單
 * 狀態 —— 它講的是這台 instance 現在的樣子。正在改 `default_role` 的那一頁可以用
 * `defaultRole` 把還沒存的值蓋進來先預覽。
 *
 * 為什麼要提醒：guest 是空權限，但拒絕是針對「帳號」不是針對 instance。匿名上傳開著
 * 的時候同一個人登出就能以陌生人的身分上傳，這個角色實際上限制不到任何人。
 */
export function GuestAnonymousWarning({
  defaultRole,
  fixAt,
}: {
  /** 這一頁表單裡還沒存的 `auth.default_role`，沒給就讀 server 上的。 */
  defaultRole?: string;
  /** 要去哪一頁修 —— 指向另外那個設定所在的頁面。 */
  fixAt: keyof typeof FIX_TARGETS;
}) {
  const { data } = useSettings();

  const role = defaultRole ?? readSetting(data, "auth.default_role");
  const anonymousUpload = readSetting(data, "features.anonymous_upload");

  if (role !== GUEST_ROLE || anonymousUpload !== true) {
    return null;
  }

  const target = FIX_TARGETS[fixAt];

  return (
    <div
      className="flex gap-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
      role="alert"
    >
      <IconAlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-500" />
      <div className="space-y-1">
        <p className="font-medium">訪客角色目前限制不到任何人</p>
        <p className="text-muted-foreground">
          新註冊帳號預設是「訪客」（不允許任何操作），但匿名上傳還開著 ——
          同一個人登出後仍可以陌生人的身分上傳。要真的擋住，請到
          <Link className="mx-1 underline underline-offset-4" to={target.to}>
            {target.label}
          </Link>
          {target.action}。
        </p>
      </div>
    </div>
  );
}
