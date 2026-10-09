import { useSession } from "@tessera/auth/hooks";
import { useTheme } from "next-themes";
import { useEffect } from "react";

import { usePreferences } from "@/hooks/use-preferences";
import {
  applyAppearance,
  readSiteAppearance,
  resolveAppearance,
} from "@/lib/appearance";

/**
 * 把「站台預設 + 這個人的偏好」解出來,寫回 `<html>`。不渲染任何東西——
 * 它是 side effect,掛在 __root 一次就好。
 *
 * 不用在 index.html 裡跑一段 script:站台預設已由伺服器直接寫在 `<html>`
 * 上(assets.rs),首次繪製就是對的顏色,沒有閃爍要擋。這裡處理的是「登入者
 * 的偏好蓋掉站台預設」,那本來就要等 session 查詢回來,不值得為它 inline script。
 *
 * 匿名訪客:`useSession` 沒登入時 `usePreferences` 關著,`personal` 是空
 * 物件,解出來等於站台預設(伺服器已寫好的那三個值),重寫一次是 no-op。
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
   * 明暗交給 next-themes,因為 `system` 要持續聽 prefers-color-scheme,它已經
   * 在做這件事。只有這個人明確選過才推給它——沒選過就讓它維持自己 localStorage
   * 的值,否則每次載入都會覆蓋使用者在別處按的切換。
   */
  useEffect(() => {
    if (isSignedIn && mode) {
      setTheme(mode);
    }
  }, [isSignedIn, mode, setTheme]);

  return null;
}
