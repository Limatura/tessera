/**
 * 外觀偏好的三個 enum。
 *
 * 這份清單必須跟三個地方一致:`crates/tessera-core/src/appearance.rs` 的
 * `Appearance`(PUT /me/preferences 和 PUT /admin/settings 拿它擋 400)、
 * 本套件的 `styles/globals.css`(每個值對應一段寫死的 CSS),以及後端
 * `Config::default()`(對應下面的 fallback)。加第八個顏色時三邊要一起改。
 *
 * 放在 `@tessera/ui` 是因為它跟 globals.css 綁在一起 —— 值改了而 CSS 沒跟上,
 * 症狀是選項選得到但畫面沒反應。同一個套件裡比較不會漏。
 */

/** `appearance.mode` */
export const APPEARANCE_MODES = ["light", "dark", "system"] as const;

/** `appearance.theme` —— 中性色盤。 */
export const APPEARANCE_THEMES = [
  "default",
  "graphite",
  "sepia",
  "matsu",
  "cosmic",
  "slate",
  "vintage",
] as const;

/** `appearance.accent` —— 強調色。 */
export const APPEARANCE_ACCENTS = [
  "violet",
  "rose",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
] as const;

export type AppearanceMode = (typeof APPEARANCE_MODES)[number];
export type AppTheme = (typeof APPEARANCE_THEMES)[number];
export type AccentColor = (typeof APPEARANCE_ACCENTS)[number];

/** 伺服器寫進 `index.html` 的站台預設,也就是沒意見的人看到的樣子。 */
export interface SiteAppearance {
  mode: AppearanceMode;
  theme: AppTheme;
  accent: AccentColor;
}

/** 三份清單都拿不到值時的最後防線,跟後端 `Config::default()` 相同。 */
export const FALLBACK_APPEARANCE: SiteAppearance = {
  accent: "orange",
  mode: "system",
  theme: "default",
};

/** 認得的值就回傳它,否則 `undefined` —— 呼叫端自己決定要不要退回預設。 */
export function memberOf<T extends string>(
  allowed: readonly T[],
  value: unknown
): T | undefined {
  return typeof value === "string" &&
    (allowed as readonly string[]).includes(value)
    ? (value as T)
    : undefined;
}
