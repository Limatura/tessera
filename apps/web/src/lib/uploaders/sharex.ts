/**
 * A ShareX custom uploader (`.sxcu`) for this instance.
 *
 * It sends the file as the raw body of `PUT /{filename}`, the same request
 * `curl -T` makes, so the instance's default expiry and limits apply. JSON is
 * asked for rather than the plain-text link because that answer ends in a
 * newline, which ShareX would copy along with the link. `path` in the JSON is
 * relative, hence the origin in front of it.
 *
 * There is no `DeletionURL`: ShareX opens one with a plain `GET` in a browser,
 * and deleting a share takes an authenticated `DELETE`.
 */
export function shareXConfig({
  name,
  origin,
  secret,
}: {
  /** Shown in ShareX's destination list. */
  name: string;
  /** Where uploads go, without a trailing slash. */
  origin: string;
  secret: string;
}): ShareXConfig {
  return {
    Body: "Binary",
    DestinationType: "ImageUploader, TextUploader, FileUploader",
    Headers: {
      Accept: "application/json",
      Authorization: `Bearer ${secret}`,
    },
    Name: name,
    RequestMethod: "PUT",
    RequestURL: `${origin}/{filename}`,
    URL: `${origin}{json:path}`,
    Version: SHAREX_CONFIG_VERSION,
  };
}

/** The file name ShareX is handed: its host, so two instances don't collide. */
export function shareXFileName(origin: string): string {
  const host = new URL(origin).host.replaceAll(UNSAFE_IN_FILE_NAME, "-");
  return `tessera-${host}.sxcu`;
}

/** The `.sxcu` format version these fields are written for. */
const SHAREX_CONFIG_VERSION = "17.0.0";

const UNSAFE_IN_FILE_NAME = /[^\w.-]/gu;

export interface ShareXConfig {
  Body: "Binary";
  DestinationType: string;
  Headers: Record<string, string>;
  Name: string;
  RequestMethod: "PUT";
  RequestURL: string;
  URL: string;
  Version: string;
}
