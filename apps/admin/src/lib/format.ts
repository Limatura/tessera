const BYTE_UNITS = ["Bytes", "KB", "MB", "GB", "TB"] as const;

export function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return "0 Bytes";
  }
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    BYTE_UNITS.length - 1
  );
  const value = bytes / 1024 ** exponent;
  const precision = exponent === 0 ? 0 : 1;
  return `${value.toFixed(precision)} ${BYTE_UNITS[exponent]}`;
}

export function formatNumber(value: number): string {
  return new Intl.NumberFormat("zh-TW").format(value);
}
