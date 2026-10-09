/**
 * 個人外觀偏好:解析、跟站台預設合併,以及套到 `<html>` 上。三個 enum 本身住在
 * `@tessera/ui/lib/appearance`,跟對應的 globals.css 放在一起。
 */

import type { SiteAppearance } from "@tessera/ui/lib/appearance";
import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  FALLBACK_APPEARANCE,
  memberOf,
} from "@tessera/ui/lib/appearance";

export {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  FALLBACK_APPEARANCE,
} from "@tessera/ui/lib/appearance";
export type {
  AccentColor,
  AppearanceMode,
  AppTheme,
  SiteAppearance,
} from "@tessera/ui/lib/appearance";

/**
 * 一個人選過的東西。三個欄位都可以缺 —— 缺 = 沒意見,持續跟著站台預設走,
 * 擁有者之後改預設,沒意見的人也會跟著變。不能用「填好預設值的完整物件」代替,
 * 那會把「沒意見」變成「剛好選了跟現在預設一樣的值」,是兩件不同的事。
 */
export interface Appearance {
  mode?: SiteAppearance["mode"];
  theme?: SiteAppearance["theme"];
  accent?: SiteAppearance["accent"];
}

/**
 * 把任意來源(伺服器回應、localStorage)的東西收斂成 `Appearance`。認不得的
 * 欄位丟掉而不是整包拒絕:這些值最後會變成 HTML 屬性,一個舊版瀏覽器存下的、
 * 這個 build 已拿掉的顏色名,不該讓整頁偏好都讀不出來。
 */
export function parseAppearance(value: unknown): Appearance {
  if (typeof value !== "object" || value === null) {
    return {};
  }
  const raw = value as Record<string, unknown>;
  const accent = memberOf(APPEARANCE_ACCENTS, raw.accent);
  const mode = memberOf(APPEARANCE_MODES, raw.mode);
  const theme = memberOf(APPEARANCE_THEMES, raw.theme);

  return {
    ...(accent ? { accent } : {}),
    ...(mode ? { mode } : {}),
    ...(theme ? { theme } : {}),
  };
}

/**
 * 伺服器寫在 `index.html` 上的站台預設,**只在模組載入時讀一次**。
 * `applyAppearance` 寫的是同樣那三個屬性,第一次套用後 DOM 上的值就是「解出來
 * 的結果」而非站台預設了;每次都重讀,會讓使用者清掉偏好時退回自己上一次選的值,
 * 而不是擁有者定的預設 —— 那正是這機制要提供的東西。
 *
 * 開發時 vite dev server 給的是原始 index.html,沒有這些屬性,退回
 * `FALLBACK_APPEARANCE`。
 */
const SITE_APPEARANCE: SiteAppearance = readSiteAppearanceFromDocument();

function readSiteAppearanceFromDocument(): SiteAppearance {
  const root = document.documentElement;

  return {
    accent:
      memberOf(APPEARANCE_ACCENTS, root.dataset.accent) ??
      FALLBACK_APPEARANCE.accent,
    mode:
      memberOf(APPEARANCE_MODES, root.dataset.mode) ?? FALLBACK_APPEARANCE.mode,
    theme:
      memberOf(APPEARANCE_THEMES, root.dataset.theme) ??
      FALLBACK_APPEARANCE.theme,
  };
}

export function readSiteAppearance(): SiteAppearance {
  return SITE_APPEARANCE;
}

/** 個人偏好蓋在站台預設上,一個欄位一個欄位地蓋。 */
export function resolveAppearance(
  site: SiteAppearance,
  personal: Appearance
): SiteAppearance {
  return {
    accent: personal.accent ?? site.accent,
    mode: personal.mode ?? site.mode,
    theme: personal.theme ?? site.theme,
  };
}

/**
 * 把解出來的結果寫到 `<html>` 上。`mode` 不在這裡處理 —— 明暗歸 next-themes
 * 管(負責 `system` 的媒體查詢監聽跟 `.dark` class),這裡只動 `data-theme` 和
 * `data-accent`,屬性不同所以不會打架。
 */
export function applyAppearance(resolved: SiteAppearance): void {
  const root = document.documentElement;
  root.dataset.theme = resolved.theme;
  root.dataset.accent = resolved.accent;
}
