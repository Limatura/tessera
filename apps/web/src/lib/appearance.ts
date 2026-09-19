/**
 * 外觀偏好的三個 enum,以及把它們套到 `<html>` 上的那一步。
 *
 * 這三份清單必須跟兩個地方一致:`crates/tessera-core` 的 `Appearance`(PUT
 * /me/preferences 拿它擋 400),以及 `packages/ui` 的 globals.css(每個值對應
 * 一段寫死的 CSS)。加第八個顏色時三邊要一起改。
 */

export const APPEARANCE_MODES = ["light", "dark", "system"] as const;
export const APP_THEMES = [
  "default",
  "graphite",
  "sepia",
  "matsu",
  "cosmic",
  "slate",
  "vintage",
] as const;
export const ACCENT_COLORS = [
  "violet",
  "rose",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
] as const;

export type AppearanceMode = (typeof APPEARANCE_MODES)[number];
export type AppTheme = (typeof APP_THEMES)[number];
export type AccentColor = (typeof ACCENT_COLORS)[number];

/**
 * 一個人選過的東西。三個欄位都可以缺 —— 缺 = 沒意見,跟著站台預設走,
 * 而且是持續跟著:擁有者之後改了站台預設,沒意見的人就跟著變。
 *
 * 所以這裡不能用「填好預設值的完整物件」代替,那會把「沒意見」變成
 * 「剛好選了跟現在預設一樣的值」,是兩件不同的事。
 */
export interface Appearance {
  mode?: AppearanceMode;
  theme?: AppTheme;
  accent?: AccentColor;
}

/** 伺服器寫進 `index.html` 的站台預設,也就是沒意見的人看到的樣子。 */
export interface SiteAppearance {
  mode: AppearanceMode;
  theme: AppTheme;
  accent: AccentColor;
}

/** 三份清單都拿不到值時的最後防線,跟後端 `Config::default()` 相同。 */
export const FALLBACK_APPEARANCE: SiteAppearance = {
  accent: "violet",
  mode: "system",
  theme: "default",
};

function memberOf<T extends string>(
  allowed: readonly T[],
  value: unknown
): T | undefined {
  return typeof value === "string" &&
    (allowed as readonly string[]).includes(value)
    ? (value as T)
    : undefined;
}

/**
 * 把任意來源(伺服器回應、localStorage)的東西收斂成 `Appearance`。
 *
 * 認不得的欄位一律丟掉而不是整包拒絕:這些值最後會變成 HTML 屬性,而一個
 * 舊版瀏覽器存下的、這個 build 已經拿掉的顏色名,不該讓整頁的偏好都讀不出來。
 */
export function parseAppearance(value: unknown): Appearance {
  if (typeof value !== "object" || value === null) {
    return {};
  }
  const raw = value as Record<string, unknown>;
  const accent = memberOf(ACCENT_COLORS, raw.accent);
  const mode = memberOf(APPEARANCE_MODES, raw.mode);
  const theme = memberOf(APP_THEMES, raw.theme);

  return {
    ...(accent ? { accent } : {}),
    ...(mode ? { mode } : {}),
    ...(theme ? { theme } : {}),
  };
}

/**
 * 伺服器寫在 `index.html` 上的站台預設。
 *
 * **在模組載入時讀一次,而且只有這一次。** `applyAppearance` 寫的是同樣那三個
 * 屬性,所以第一次套用之後 DOM 上的值就是「解出來的結果」而不是站台預設了;
 * 每次都重讀會讓使用者清掉某個偏好時退回自己上一次選的值,而不是擁有者定的
 * 預設 —— 那正是這整個機制要提供的東西。
 *
 * 開發時跑 vite dev server 拿到的是原始 index.html,沒有這些屬性,就退回
 * `FALLBACK_APPEARANCE`。
 */
const SITE_APPEARANCE: SiteAppearance = readSiteAppearanceFromDocument();

function readSiteAppearanceFromDocument(): SiteAppearance {
  const root = document.documentElement;

  return {
    accent:
      memberOf(ACCENT_COLORS, root.dataset.accent) ??
      FALLBACK_APPEARANCE.accent,
    mode:
      memberOf(APPEARANCE_MODES, root.dataset.mode) ?? FALLBACK_APPEARANCE.mode,
    theme:
      memberOf(APP_THEMES, root.dataset.theme) ?? FALLBACK_APPEARANCE.theme,
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
 * 把解出來的結果寫到 `<html>` 上。
 *
 * `mode` 不在這裡處理 —— 明暗歸 next-themes 管(它要負責 `system` 的媒體查詢
 * 監聽跟 `.dark` class),這裡只動 `data-theme` 和 `data-accent`,兩邊寫的是
 * 不同屬性所以不會打架。
 */
export function applyAppearance(resolved: SiteAppearance): void {
  const root = document.documentElement;
  root.dataset.theme = resolved.theme;
  root.dataset.accent = resolved.accent;
}
