/**
 * The address an uploader config should send files to.
 *
 * A config outlives the page that wrote it, so it wants the address the
 * operator configured (`server.public_url`) rather than whatever host this
 * browser reached the page through, which behind a proxy can be an internal
 * name. Without one, the page's own origin is the best guess there is.
 */
export function uploaderOrigin(
  publicUrl: string | null | undefined,
  pageOrigin: string
): string {
  const configured = publicUrl?.trim().replace(TRAILING_SLASHES, "");
  return configured || pageOrigin.replace(TRAILING_SLASHES, "");
}

const TRAILING_SLASHES = /\/+$/u;
