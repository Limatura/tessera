import type { components } from "@tessera/api/schema";

import { readSetting } from "@/hooks/use-settings";

type Settings = components["schemas"]["SettingsResponse"];

/**
 * `GET /admin/settings` 的 `value` 是不透明的 JSON —— 型別要看是哪個 key，所以
 * 每個讀取都得自己比對。拿不到（還在載入、這版 server 沒這個 key、或這是 SECRET
 * 所以永遠是 null）一律退回 fallback。
 */
export function asString(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

export function asNumber(value: unknown, fallback: number): number {
  return typeof value === "number" ? value : fallback;
}

export function asBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

/** `scopes` 之類的 `string[]`。表單用空白分隔的單行字串編輯。 */
export function asScopes(value: unknown, fallback: string): string {
  if (!Array.isArray(value)) {
    return fallback;
  }
  return value.filter((item) => typeof item === "string").join(" ");
}

/** 把表單上那行空白分隔的字串轉回 server 要的 `string[]`。 */
export function toScopes(value: string): string[] {
  return value.split(/\s+/u).filter(Boolean);
}

export function asEnum<const TOptions extends readonly string[]>(
  value: unknown,
  options: TOptions,
  fallback: TOptions[number]
): TOptions[number] {
  return options.find((option) => option === value) ?? fallback;
}

/**
 * SEALED 欄位回來的是「有沒有設過」而不是值本身，所以只夠拿來決定 placeholder 寫
 * 「已設定」還是「未設定」。
 */
export function isSecretSet(
  settings: Settings | undefined,
  key: string
): boolean {
  return readSetting(settings, key) === true;
}

/** `trusted_proxies` 之類的 `string[]`，表單用一行一筆編輯。 */
export function asLines(value: unknown, fallback: string): string {
  if (!Array.isArray(value)) {
    return fallback;
  }
  return value.filter((item) => typeof item === "string").join("\n");
}

/** 把多行文字轉回 server 要的 `string[]`，去掉空行與前後空白。 */
export function toLines(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}
