import type { components } from "@tessera/api/schema";

export type ShareResponse = components["schemas"]["ShareResponse"];
export type ErrorBody = components["schemas"]["ErrorBody"];
type PatchShareRequest = components["schemas"]["PatchShareRequest"];

export interface UploadResultData {
  code: string;
  fileName: string;
  shortUrl: string;
  /** 從現在起還剩幾分鐘;`undefined` 是永不過期 */
  expiresInMinutes?: number;
  maxUses?: number;
  hasPassword: boolean;
  /** Only present for an anonymous upload; lets the uploader delete it back. */
  managementToken?: string;
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
}

/**
 * 失敗時丟的 `Error`。`message` 是 server 的英文（沒有回應本體時是狀態碼），
 * 另外掛上 `code`、`details`，畫面才能用 `apiErrorMessage` 翻譯。
 */
export function apiError(body: ErrorBody | null, fallback: string): Error {
  const error = new Error(body?.message ?? fallback);
  return body
    ? Object.assign(error, { code: body.code, details: body.details })
    : error;
}

export interface ShareQuery {
  /** 只有連結分享要帶;檔案跟文字由其他參數區分。 */
  kind?: "link";
  filename?: string;
  title?: string;
  language?: string;
  expires_in_hours?: number;
  max_uses?: number;
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
 * `DELETE /api/v1/shares/{code}` 需要登入的 session 或匿名上傳時拿到的
 * `management_token`（透過 `X-Share-Management-Token` header 傳遞）
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
 * `PATCH /api/v1/shares/{code}` 的授權方式跟 DELETE 一樣:登入的 session,
 * 或匿名上傳時拿到的 `management_token`。
 *
 * 三個欄位都是「省略 = 不動這項設定」,跟「送 null = 清掉」不一樣,所以下面
 * 只把呼叫端真的有指定的欄位放進 body。
 *
 * 密碼是特例:設定密碼走 `X-Share-Password` header(不進 query string、
 * access log 和 Referer),移除密碼才走 body 的 `remove_password`。後端明講
 * 兩個一起送會被拒絕,所以這裡二選一。
 */
export async function updateShare(
  code: string,
  settings: ShareSettings,
  managementToken?: string
): Promise<void> {
  const body: PatchShareRequest = {};
  if ("expiresInMinutes" in settings) {
    // 0 在建立時代表「永不過期」,PATCH 這邊要用 null 表達。
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
 * `fetch` 沒有上傳進度事件，所以改用 `XMLHttpRequest`
 * ——進度只能透過回呼來取得，因此需要這個本來可以避免的 `Promise` wrapper
 */
export function createShare(
  body: Blob | string,
  query: ShareQuery,
  options: UploadOptions,
  basePath = "/api/v1/shares",
  managementToken?: string
): Promise<ShareResponse> {
  // oxlint-disable-next-line promise/avoid-new -- XHR 的進度事件只能透過回呼取得
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
