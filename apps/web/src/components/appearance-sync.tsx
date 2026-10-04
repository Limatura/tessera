import { useTheme } from "next-themes";
import { useEffect } from "react";

import { useSession } from "@/hooks/use-auth";
import { usePreferences } from "@/hooks/use-preferences";
import {
  applyAppearance,
  readSiteAppearance,
  resolveAppearance,
} from "@/lib/appearance";

/**
 * 把「站台預設 + 這個人的偏好」解出來,寫回 `<html>`。
 *
 * 不渲染任何東西 —— 它做的事是 side effect,掛在 __root 一次就好。
 *
 * # 為什麼不是在 index.html 裡跑一段 script
 *
 * 不需要。站台預設已經由伺服器直接寫在 `<html>` 上(assets.rs),所以首次繪製
 * 就是對的顏色,沒有閃爍要擋。這裡處理的是「登入者的偏好蓋掉站台預設」,那個
 * 本來就要等 session 查詢回來,擋不掉也不值得為它 inline 一段 script。
 *
 * # 匿名訪客
 *
 * `useSession` 沒登入時 `usePreferences` 是關著的,`personal` 就是空物件,
 * 解出來等於站台預設 —— 也就是伺服器已經寫好的那三個值,重寫一次是 no-op。
 */
export function AppearanceSync() {
  const { data: session } = useSession();
  const isSignedIn = Boolean(session);
  // 強制改密碼期間偏好端點一律 403，先照站台預設走，改完 session 一變就會抓。
  const { data: personal } = usePreferences(
    isSignedIn && !session?.must_change_password
  );
  const { setTheme } = useTheme();

  const mode = personal?.mode;
  const theme = personal?.theme;
  const accent = personal?.accent;

  useEffect(() => {
    const site = readSiteAppearance();
    applyAppearance(
      resolveAppearance(site, isSignedIn ? { accent, mode, theme } : {})
    );
  }, [accent, isSignedIn, mode, theme]);

  /*
   * 明暗交給 next-themes,因為 `system` 要持續聽 prefers-color-scheme,而那是
   * 它已經在做的事。只有這個人明確選過才推給它 —— 沒選過就讓它維持自己
   * localStorage 裡的值,不然每次載入都會把使用者在別處按的切換覆蓋掉。
   */
  useEffect(() => {
    if (isSignedIn && mode) {
      setTheme(mode);
    }
  }, [isSignedIn, mode, setTheme]);

  return null;
}
