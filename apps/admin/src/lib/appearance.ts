import { api } from "@tessera/api";

import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
} from "@/lib/settings-schema";
import { asEnum } from "@/lib/settings-values";

/**
 * 讓後台自己也套上站台外觀。
 *
 * # 為什麼 admin 要自己做這件事
 *
 * `apps/web` 的顏色是伺服器在送出 `index.html` 時直接寫進 `<html>` 的
 * (`assets.rs` 的 `with_appearance`),所以首次繪製就是對的。後台不是從那個
 * binary 出去的 —— 它有自己的 Dockerfile 和 nginx,拿到的是原封不動的
 * `index.html`,那段 splice 根本不經過它。
 *
 * # 為什麼不是 TanStack Router 的 beforeLoad
 *
 * 擋不到。`beforeLoad` 最早也要等 `RouterProvider` 掛載才會跑,而閃爍發生在
 * 那之前 —— 瀏覽器拿到 HTML 就先畫一次了。而且根路由的 component 永遠會被
 * 渲染(router 的 FAQ 明說不能條件式地不渲染它),`beforeLoad` 只擋得住子路由
 * 的資料載入,擋不住「已經畫出來的那一幀」。
 *
 * 所以唯一趕得上的時機是 `main.tsx` 裡 `createRoot` 之前,也就是這個函式。
 *
 * # 代價:多一個 round trip 才畫第一幀
 *
 * 這是拿「晚一點畫」換「不要畫錯再改」。在後台這個取捨是划算的:它是操作者
 * 每天開著的內部工具,不是要衝首次繪製時間的公開頁面,而深色實例上那一下白閃
 * 是唯一會被注意到的東西。
 *
 * 失敗(沒權限、離線、`features.admin` 關著)就什麼都不做,維持預設色盤 ——
 * 上不了色不該是進不了後台的理由。
 */
export async function applySiteAppearance(): Promise<void> {
  try {
    const { data, error } = await api.GET("/api/v1/admin/settings");
    if (error || !data) {
      return;
    }

    const read = (key: string) =>
      data.settings.find((setting) => setting.key === key)?.value;

    const root = document.documentElement;
    root.dataset.theme = asEnum(
      read("appearance.theme"),
      APPEARANCE_THEMES,
      "default"
    );
    root.dataset.accent = asEnum(
      read("appearance.accent"),
      APPEARANCE_ACCENTS,
      "violet"
    );
    /*
     * `mode` 寫成 `data-mode` 給 `__root.tsx` 的 ThemeProvider 當 defaultTheme
     * 讀,不在這裡直接加 `.dark` class —— 那是 next-themes 的地盤,它要負責
     * `system` 的媒體查詢監聽,兩邊都寫會打架。
     */
    root.dataset.mode = asEnum(
      read("appearance.mode"),
      APPEARANCE_MODES,
      "system"
    );
  } catch {
    // 見上:上色失敗就維持預設,不擋後台開起來。
  }
}
