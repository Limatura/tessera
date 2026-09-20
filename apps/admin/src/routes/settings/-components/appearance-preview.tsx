import { cn } from "@tessera/ui/lib/utils";

import type { AppearanceSettingsValues } from "@/lib/settings-schema";

const MODE_LABELS: Record<AppearanceSettingsValues["mode"], string> = {
  dark: "深色",
  light: "淺色",
  system: "跟隨裝置",
};

/**
 * 表單目前那三個值合起來長什麼樣。
 *
 * # 為什麼是一塊預覽而不是直接套到整個後台
 *
 * 這一頁改的是**訪客**看到的樣子,不是操作者自己的主題(那是同一頁上面那個
 * 走 next-themes 的欄位)。把值套到整個後台會讓兩件事看起來是同一件,而且
 * 操作者一選就發現自己的介面被改掉了 —— 那不是他要求的。
 *
 * 所以把 `data-theme` / `data-accent` 放在這個 div 上。globals.css 的規則是
 * 屬性選擇器而不是 `:root`,所以在子樹上一樣生效,裡面的 `bg-card`、
 * `bg-primary` 讀到的就是選中的那組值。
 *
 * # 明暗只預覽得到一半,這是刻意的
 *
 * 深色是 `.dark` 祖先 + `@custom-variant dark (&:is(.dark *))`,所以在後台是
 * 深色時,這塊子樹**沒辦法**變回淺色 —— 後代選擇器不能從內部取消。與其為了
 * 這塊預覽去複製一份淺色 token(第二份會跟 globals.css 漂移的清單),不如只在
 * 後台本來就是淺色時才預覽得到淺色,並在下面用文字說明選了什麼。
 *
 * 色盤和強調色不受這個限制,它們才是這一頁真正在選的東西。
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
        預覽 · {MODE_LABELS[values.mode]}
      </p>
      <div className="rounded-md border bg-card p-3 text-card-foreground">
        <p className="font-medium text-sm">分享已建立</p>
        <p className="mt-1 text-muted-foreground text-xs">
          訪客會看到這組顏色，直到他們自己選過為止。
        </p>
        <div className="mt-3 flex items-center gap-2">
          <span className="rounded-md bg-primary px-3 py-1.5 font-medium text-primary-foreground text-xs">
            下載
          </span>
          <span className="rounded-md border px-3 py-1.5 text-xs">取消</span>
          <span className="ml-auto size-4 rounded-full bg-primary" />
        </div>
      </div>
    </div>
  );
}
