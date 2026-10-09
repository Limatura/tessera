import { cn } from "@tessera/ui/lib/utils";

import type { AppearanceSettingsValues } from "@/lib/admin/settings-schema";
import * as m from "@/paraglide/messages";

/**
 * 是函式而不是 module-level const —— 裡面呼叫的 `m.xxx()` 要在渲染當下讀 locale，
 * 包成 const 只會在模組第一次載入時凍住語系。
 */
function modeLabels(): Record<AppearanceSettingsValues["mode"], string> {
  return {
    dark: m.settings_appearance_preview_mode_dark(),
    light: m.settings_appearance_preview_mode_light(),
    system: m.settings_appearance_preview_mode_system(),
  };
}

/**
 * 表單目前那三個值合起來長什麼樣。
 *
 * 只套在這塊 div,不套到整個後台:這一頁改的是**訪客**看到的樣子,不是操作者
 * 自己的主題(同頁上方另一個走 next-themes 的欄位)。套到整個後台會讓操作者
 * 一選就發現自己的介面被改掉,混淆兩件事。`data-theme` / `data-accent` 能只
 * 影響子樹,是因為 globals.css 的規則是屬性選擇器而非 `:root`。
 *
 * 明暗只預覽得到一半,是刻意的:深色靠 `.dark` 祖先 + `@custom-variant dark
 * (&:is(.dark *))`,後代選擇器沒辦法從內部取消它,所以後台本身是深色時這塊
 * 子樹變不回淺色。與其為此複製一份淺色 token(多一份會跟 globals.css 漂移的
 * 清單),不如只在後台本來就是淺色時才預覽得到淺色,下面用文字補充說明選了
 * 什麼。色盤和強調色不受此限,它們才是這一頁真正在選的東西。
 */
export function AppearancePreview({
  values,
}: {
  values: AppearanceSettingsValues;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border bg-background p-4 text-foreground",
        values.mode === "dark" && "dark"
      )}
      data-accent={values.accent}
      data-theme={values.theme}
    >
      <p className="mb-3 font-medium text-muted-foreground text-xs">
        {m.settings_appearance_preview_heading({
          mode: modeLabels()[values.mode],
        })}
      </p>
      <div className="rounded-md border bg-card p-3 text-card-foreground">
        <p className="font-medium text-sm">
          {m.settings_appearance_preview_card_title()}
        </p>
        <p className="mt-1 text-muted-foreground text-xs">
          {m.settings_appearance_preview_card_description()}
        </p>
        <div className="mt-3 flex items-center gap-2">
          <span className="rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground text-xs">
            {m.settings_appearance_preview_download()}
          </span>
          <span className="rounded-md border px-3 py-1.5 text-xs">
            {m.settings_appearance_preview_cancel()}
          </span>
          <span className="ml-auto size-4 rounded-full bg-primary" />
        </div>
      </div>
    </div>
  );
}
