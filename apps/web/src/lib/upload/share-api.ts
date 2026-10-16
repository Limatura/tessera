import type { components } from "@tessera/api/schema";

import type { EncryptionChoice } from "./encryption";

export type ShareResponse = components["schemas"]["ShareResponse"];
export type ErrorBody = components["schemas"]["ErrorBody"];
type PatchShareRequest = components["schemas"]["PatchShareRequest"];

export interface UploadResultData {
  code: string;
  fileName: string;
  shortUrl: string;
  /** Minutes left from now; `undefined` never expires. */
  expiresInMinutes?: number;
  maxUses?: number;
  hasPassword: boolean;
  /** Only present for an anonymous upload; lets the uploader delete it back. */
  managementToken?: string;
  /** No scan verdict yet; the recipient cannot open it until there is one. */
  scanning: boolean;
  /** The scanner's verdict at creation; `null` when scanning is off. See `ShareResponse.scan_state`. */
  scanState: string | null;
  /**
   * End-to-end encrypted with a key from a password: the share's password is
   * that key's token, so it cannot be changed or removed.
   */
  passwordIsKey?: boolean;
  /** Sealed in this browser before it was sent. */
  encrypted?: boolean;
}

export interface ShareSettings {
  /** Minutes from now. `undefined` or `0` never expires. */
  expiresInMinutes?: number;
  /** Retrievals allowed from now on; `1` is burn-after-reading. Omit for unlimited. */
  maxUses?: number;
  password?: string;
}

export interface UploadOptions {
  onProgress?: (percent: number) => void;
  signal?: AbortSignal;
  /** Seal the share before it leaves this browser. Not for a link share. */
  encryption?: EncryptionChoice;
}

/**
 * The `Error` thrown on failure. `message` is the server's English (or the
 * status code when there is no body); `code` and `details` are attached so the
 * screen can translate it with `apiErrorMessage`.
 */
export function apiError(body: ErrorBody | null, fallback: string): Error {
  const error = new Error(body?.message ?? fallback);
  return body
    ? Object.assign(error, { code: body.code, details: body.details })
    : error;
}

export interface ShareQuery {
  /** Only for a link share; files and text are told apart by other parameters. */
  kind?: "link";
  filename?: string;
  title?: string;
  language?: string;
  expires_in_hours?: number;
  max_uses?: number;
  /** `link` or `password` for an end-to-end encrypted share. */
  encryption?: "link" | "password";
  /** For `encryption=password`: the salt the key was derived with. */
  key_salt?: string;
  /** For an encrypted file: its name and type, sealed. Replaces `filename`. */
  sealed_name?: string;
  /** For an encrypted share: its title and language, sealed. */
  sealed_details?: string;
}

export function toResult(share: ShareResponse): UploadResultData {
  return {
    code: share.code,
    expiresInMinutes: minutesUntil(share.expires_at),
    fileName: share.files[0]?.filename ?? share.title ?? share.code,
    hasPassword: share.has_password,
    managementToken: share.management_token ?? undefined,
    maxUses:
      share.uses_remaining === null || share.uses_remaining === undefined
        ? undefined
        : share.use_count + share.uses_remaining,
    scanState: share.scan_state ?? null,
    scanning: share.scanning,
    shortUrl: new URL(share.path, window.location.origin).toString(),
  };
}

export function minutesUntil(
  expiresAt: string | null | undefined
): number | undefined {
  if (!expiresAt) {
    return;
  }
  const ms = new Date(expiresAt).getTime() - Date.now();
  return Math.max(0, Math.round(ms / (60 * 1000)));
}

/**
 * `DELETE /api/v1/shares/{code}` needs a signed-in session or the
 * `management_token` from an anonymous upload (sent in the
 * `X-Share-Management-Token` header).
 */
export async function deleteShare(
  code: string,
  managementToken?: string
): Promise<void> {
  const headers: HeadersInit = {};
  if (managementToken) {
    headers["X-Share-Management-Token"] = managementToken;
  }
  const response = await fetch(`/api/v1/shares/${encodeURIComponent(code)}`, {
    credentials: "include",
    headers,
    method: "DELETE",
  });
  if (!response.ok) {
    const errorBody = (await response
      .json()
      .catch(() => null)) as ErrorBody | null;
    throw apiError(errorBody, `Delete failed: ${response.status}`);
  }
}

/**
 * `PATCH /api/v1/shares/{code}` is authorised like DELETE: a signed-in
 * session, or the `management_token` from an anonymous upload.
 *
 * Every field means "leave unchanged" when omitted, unlike "clear it" when
 * sent as null, so only the fields the caller actually set go into the body.
 *
 * The password is the exception: setting it uses the `X-Share-Password`
 * header (kept out of the query string, access logs and Referer), and only
 * removing it uses `remove_password` in the body. The server refuses both
 * together, so this sends one or the other.
 */
export async function updateShare(
  code: string,
  settings: ShareSettings,
  managementToken?: string
): Promise<void> {
  const body: PatchShareRequest = {};
  if ("expiresInMinutes" in settings) {
    // On creation 0 means "never expires"; PATCH says that with null.
    body.expires_in_minutes =
      settings.expiresInMinutes === undefined || settings.expiresInMinutes === 0
        ? null
        : settings.expiresInMinutes;
  }
  if ("maxUses" in settings) {
    body.max_uses = settings.maxUses ?? null;
  }

  const headers: HeadersInit = { "Content-Type": "application/json" };
  if (managementToken) {
    headers["X-Share-Management-Token"] = managementToken;
  }
  if ("password" in settings) {
    if (settings.password === undefined) {
      body.remove_password = true;
    } else {
      headers["X-Share-Password"] = settings.password;
    }
  }

  const response = await fetch(`/api/v1/shares/${encodeURIComponent(code)}`, {
    body: JSON.stringify(body),
    credentials: "include",
    headers,
    method: "PATCH",
  });
  if (!response.ok) {
    const errorBody = (await response
      .json()
      .catch(() => null)) as ErrorBody | null;
    throw apiError(errorBody, `Update failed: ${response.status}`);
  }
}

function buildShareUrl(query: ShareQuery, basePath = "/api/v1/shares"): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) {
      params.set(key, String(value));
    }
  }
  const search = params.toString();
  return `${basePath}${search ? `?${search}` : ""}`;
}

/**
 * `fetch` has no upload progress events, so this uses `XMLHttpRequest`, whose
 * progress arrives only through callbacks; hence the `Promise` wrapper.
 */
export function createShare(
  body: Blob | string,
  query: ShareQuery,
  options: UploadOptions,
  basePath = "/api/v1/shares",
  managementToken?: string
): Promise<ShareResponse> {
  // oxlint-disable-next-line promise/avoid-new -- XHR progress is only available through callbacks
  return new Promise((resolve, reject) => {
    if (options.signal?.aborted) {
      reject(
        options.signal.reason ?? new DOMException("Aborted", "AbortError")
      );
      return;
    }

    const xhr = new XMLHttpRequest();
    xhr.open("POST", buildShareUrl(query, basePath));
    xhr.responseType = "json";
    xhr.withCredentials = true;
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    if (managementToken) {
      xhr.setRequestHeader("X-Share-Management-Token", managementToken);
    }

    const handleAbort = () => xhr.abort();
    options.signal?.addEventListener("abort", handleAbort);
    const cleanup = () =>
      options.signal?.removeEventListener("abort", handleAbort);

    xhr.upload.addEventListener("progress", (event) => {
      if (event.lengthComputable) {
        options.onProgress?.((event.loaded / event.total) * 100);
      }
    });

    xhr.addEventListener("abort", () => {
      cleanup();
      reject(new DOMException("Upload cancelled", "AbortError"));
    });
    xhr.addEventListener("error", () => {
      cleanup();
      reject(new Error("Network error during upload"));
    });
    xhr.addEventListener("load", () => {
      cleanup();
      if (xhr.status >= 200 && xhr.status < 300) {
        resolve(xhr.response as ShareResponse);
        return;
      }
      reject(
        apiError(
          xhr.response as ErrorBody | null,
          `Upload failed: ${xhr.status}`
        )
      );
    });

    xhr.send(body);
  });
}
