import { toast } from "@tessera/ui/components/toast";

import { usePreferences, useUpdatePreferences } from "@/hooks/use-preferences";
import type { Appearance } from "@/lib/appearance";
import { readSiteAppearance, resolveAppearance } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

/**
 * 三個外觀欄位共用的那一段:目前顯示什麼、按下去送什麼。
 *
 * 顯示的是**解出來的值**(站台預設被個人偏好蓋過之後),不是個人偏好本身 ——
 * 沒選過的人應該看到自己實際看到的那個顏色是被選取的,而不是三個都沒選取。
 *
 * 送的是**整包** `Appearance`:端點是 PUT,少送的欄位會被當成沒意見清掉,所以
 * 改一個欄位也要把另外兩個帶上。這裡刻意帶「個人偏好」的另外兩個而不是解出來的
 * 值 —— 否則改個強調色會順手把「跟著站台的明暗」定死成當下的值。
 */
export function useAppearancePreference() {
  const { data: personal } = usePreferences();
  const update = useUpdatePreferences();

  const current = resolveAppearance(readSiteAppearance(), personal ?? {});

  const choose = (change: Appearance) => {
    update.mutate(
      { ...personal, ...change },
      {
        onError: () =>
          toast.add({ title: m.user_prefs_save_failed(), type: "error" }),
      }
    );
  };

  return { choose, current, isPending: update.isPending };
}
