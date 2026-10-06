import type { components } from "@tessera/api/schema";
import type * as React from "react";

import { useSettings } from "@/hooks/admin/use-settings";
import * as m from "@/paraglide/messages";

type Settings = components["schemas"]["SettingsResponse"];

/**
 * 先把設定讀回來，再把表單掛上去。
 *
 * 表單的初始值只在掛載時讀一次，所以掛載前一定要有資料 —— 讓表單自己處理
 * `undefined` 的話，第一次 render 的預設值會是空的，使用者按下儲存就把真正的設定
 * 洗掉了。
 */
export function SettingsLoader({
  children,
}: {
  children: (settings: Settings) => React.ReactNode;
}) {
  const settings = useSettings();

  if (settings.isPending) {
    return (
      <p className="text-muted-foreground text-sm">
        {m.settings_loader_loading()}
      </p>
    );
  }

  if (settings.isError || !settings.data) {
    return (
      <p className="text-destructive text-sm">{m.settings_loader_error()}</p>
    );
  }

  return children(settings.data);
}
