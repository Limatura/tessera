import type { components } from "@tessera/api/schema";

export interface UploadResultData {
  code: string;
  fileName: string;
  shortUrl: string;
  expiresInHours?: number;
  maxUses?: number;
  hasPassword: boolean;
  /** Only present for an anonymous upload; lets the uploader delete it back. */
  managementToken?: string;
}

export interface ShareSettings {
  /** Omit/undefined to use the instance default; `0` never expires. */
  expiresInHours?: number;
  /** `1` is burn-after-reading. Omit for unlimited. */
  maxUses?: number;
  password?: string;
}

export interface UploadOptions {
  onProgress?: (percent: number) => void;
  signal?: AbortSignal;
}

export interface FileUploadPayload {
  type: "file";
  file: File;
}

export interface PasteUploadPayload {
  type: "paste";
  content: string;
  /** Present when submitted from the code tab; absent for plain text. */
  language?: string;
}

export interface LinkUploadPayload {
  type: "link";
  url: string;
}

export type UploadPayload =
  | FileUploadPayload
  | PasteUploadPayload
  | LinkUploadPayload;

type ShareResponse = components["schemas"]["ShareResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

export function upload(
  payload: UploadPayload,
  options: UploadOptions = {}
): Promise<UploadResultData> {
  switch (payload.type) {
    case "file": {
      return uploadFile(payload, options);
    }
    case "paste": {
      return uploadPaste(payload, options);
    }
    case "link": {
      return uploadLink(payload, options);
    }
    default: {
      const exhaustiveCheck: never = payload;
      throw new Error(`Unsupported upload type: ${exhaustiveCheck}`);
    }
  }
}

function uploadFile(
  payload: FileUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  return createShare(
    payload.file,
    { filename: payload.file.name },
    options
  ).then(toResult);
}

function uploadPaste(
  payload: PasteUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  return createShare(
    payload.content,
    { language: payload.language },
    options
  ).then(toResult);
}

function uploadLink(
  payload: LinkUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  // 後端沒有 URL 縮短器/重新導向分享方式，連結會變成純文字分享，內容就是 URL 本身
  return createShare(payload.url, { title: payload.url }, options).then(
    toResult
  );
}

function toResult(share: ShareResponse): UploadResultData {
  return {
    code: share.code,
    expiresInHours: hoursUntil(share.expires_at),
    fileName: share.filename ?? share.title ?? share.code,
    hasPassword: share.has_password,
    managementToken: share.management_token ?? undefined,
    maxUses:
      share.uses_remaining === null || share.uses_remaining === undefined
        ? undefined
        : share.use_count + share.uses_remaining,
    shortUrl: new URL(share.path, window.location.origin).toString(),
  };
}

function hoursUntil(expiresAt: string | null | undefined): number | undefined {
  if (!expiresAt) {
    return;
  }
  const ms = new Date(expiresAt).getTime() - Date.now();
  return Math.max(0, Math.round(ms / (60 * 60 * 1000)));
}

interface ShareQuery {
  filename?: string;
  title?: string;
  language?: string;
  expires_in_hours?: number;
  max_uses?: number;
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
    throw new Error(errorBody?.message ?? `Delete failed: ${response.status}`);
  }
}

/**
 * 讓使用者在上傳完成後調整過期時間、下載即焚、密碼。
 *
 * 後端目前只有 `POST /shares`（建立）跟 `DELETE /shares/{code}`（刪除），
 * 沒有可以修改既有分享設定的 endpoint，也沒有密碼參數。等後端補上
 * `PATCH /api/v1/shares/{code}` 後，把下面換成真正的 fetch 呼叫；在那之前
 * 直接視為成功，讓畫面上的互動可以測試。
 */
export function updateShare(
  code: string,
  settings: ShareSettings,
  managementToken?: string
): Promise<void> {
  void code;
  void settings;
  void managementToken;
  return Promise.resolve();
}

function buildShareUrl(query: ShareQuery): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) {
      params.set(key, String(value));
    }
  }
  const search = params.toString();
  return `/api/v1/shares${search ? `?${search}` : ""}`;
}

/**
 * `fetch` 沒有上傳進度事件，所以改用 `XMLHttpRequest`
 * ——進度只能透過回呼來取得，因此需要這個本來可以避免的 `Promise` wrapper
 */
function createShare(
  body: Blob | string,
  query: ShareQuery,
  options: UploadOptions
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
    xhr.open("POST", buildShareUrl(query));
    xhr.responseType = "json";
    xhr.withCredentials = true;
    xhr.setRequestHeader("Content-Type", "application/octet-stream");

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
      const errorBody = xhr.response as ErrorBody | null;
      reject(new Error(errorBody?.message ?? `Upload failed: ${xhr.status}`));
    });

    xhr.send(body);
  });
}
