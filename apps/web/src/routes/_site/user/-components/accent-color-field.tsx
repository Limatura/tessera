import { AccentPicker } from "@tessera/ui/components/theme-picker";

import { useAppearancePreference } from "@/hooks/use-appearance-preference";
import { APPEARANCE_ACCENTS } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

/**
 * 強調色。色票由 `@tessera/ui` 的 `AccentPicker` 畫,每顆自己帶 `data-accent`
 * 並用 `bg-primary`,所以看到的就是按下去會變成的顏色。
 *
 * 選項直接從 `APPEARANCE_ACCENTS` 展開而不是自己抄一份:那份清單已經要跟後端的
 * `Accent` enum 和 globals.css 對齊,再抄第三份只是多一個會漂移的地方。
 */
export function AccentColorField() {
  const { choose, current, isPending } = useAppearancePreference();

  const options = APPEARANCE_ACCENTS.map((value) => ({
    label: m.user_prefs_accent_select({ color: value }),
    value,
  }));

  return (
    <div className="space-y-3">
      <div>
        <p className="font-medium text-sm">{m.user_prefs_accent_title()}</p>
        <p className="text-muted-foreground text-sm">
          {m.user_prefs_accent_description()}
        </p>
      </div>
      <AccentPicker
        disabled={isPending}
        onSelect={(accent) => choose({ accent })}
        options={options}
        value={current.accent}
      />
    </div>
  );
}
