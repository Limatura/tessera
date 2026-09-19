const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
const BYTES_PER_UNIT = 1024;

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
