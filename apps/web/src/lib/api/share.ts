/** Where a ticket actually fetches the bytes: inline text or an attachment. */
export function buildTransferUrl(
  code: string,
  endpoint: "download" | "raw",
  ticket: string
) {
  return `/s/${code}/${endpoint}?t=${encodeURIComponent(ticket)}`;
}
