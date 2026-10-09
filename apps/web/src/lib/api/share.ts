/** 票券實際取得位元組的地方：內聯文字或附件 */
export function buildTransferUrl(
  code: string,
  endpoint: "download" | "raw",
  ticket: string
) {
  return `/s/${code}/${endpoint}?t=${encodeURIComponent(ticket)}`;
}
