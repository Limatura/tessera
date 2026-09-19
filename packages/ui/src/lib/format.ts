const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
const BYTES_PER_UNIT = 1024;

/**
 * 檔案大小。整數位元組不加小數點(`999 B`),進位之後固定一位(`1.5 KB`)。
 *
 * 負數跟 0 一律回 `0 B`:`Math.log` 對負數是 NaN,而「負的檔案大小」本來就
 * 只會是上游算錯,顯示成 0 比顯示 NaN 有用。
 */
export function formatBytes(bytes: number): string {
  if (bytes <= 0) {
    return "0 B";
  }

  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(BYTES_PER_UNIT)),
    BYTE_UNITS.length - 1
  );
  const value = bytes / BYTES_PER_UNIT ** exponent;

  return `${exponent === 0 ? value : value.toFixed(1)} ${BYTE_UNITS[exponent]}`;
}

/**
 * 千分位。
 *
 * `locale` 必須傳:寫死任何一個語言都會讓另外兩種介面語言看到不一致的數字格式。
 * 呼叫端手上一定有 `getLocale()`。
 */
export function formatNumber(value: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(value);
}

/**
 * 日期時間格式器,跟著介面語言與偏好設定裡的時間格式走。
 *
 * `auto` 就是不指定 `hour12`,讓 `Intl` 依 locale 決定 —— 這跟「12 小時制」不同,
 * 寫死 `hour12: false` 會讓 zh-TW 的使用者看到跟系統其他地方不一樣的時間。
 */
export function createDateTimeFormatter(
  locale: string,
  timeFormat: "auto" | "12h" | "24h"
) {
  const formatter = new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    hour12: timeFormat === "auto" ? undefined : timeFormat === "12h",
    timeStyle: "short",
  });

  return (value: string | null | undefined) =>
    value ? formatter.format(new Date(value)) : "—";
}
