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

export interface FileEntry {
  file: File;
  /** Relative path within the dropped/selected folder, `/`-separated, no leading slash. */
  path: string;
}

export interface FileUploadPayload {
  type: "file";
  files: FileEntry[];
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
type PatchShareRequest = components["schemas"]["PatchShareRequest"];
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

async function uploadFile(
  payload: FileUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  const { files } = payload;
  const total = files.length;
  // 已完成的檔案數加上目前這個檔案的位元組進度，合成成整體百分比
  const reportProgress = (done: number, currentFilePercent: number) => {
    options.onProgress?.(((done + currentFilePercent / 100) / total) * 100);
  };

  const [first, ...rest] = files;
  if (!first) {
    throw new Error("No file selected");
  }

  let share = await createShare(
    first.file,
    { filename: first.path || first.file.name },
    {
      onProgress: (percent) => reportProgress(0, percent),
      signal: options.signal,
    }
  );
  // 只有建立時才會回傳，之後每個 add_file 的回應都不會再帶，所以要記下來重複使用
  const managementToken = share.management_token ?? undefined;
  reportProgress(1, 0);

  try {
    for (const [index, entry] of rest.entries()) {
      const done = index + 1;
      // 每個檔案都要掛到同一個 share 上，且進度要照實際完成順序回報，不能平行送出
      // oxlint-disable-next-line no-await-in-loop -- 見上一行說明
      share = await addFileToShare(
        share.code,
        entry.file,
        entry.path || entry.file.name,
        managementToken,
        {
          onProgress: (percent) => reportProgress(done, percent),
          signal: options.signal,
        }
      );
      reportProgress(done + 1, 0);
    }
  } catch (error) {
    await deleteShare(share.code, managementToken).catch(() => {
      // 已經在回報原本的錯誤，刪除失敗就不再蓋過去
    });
    throw error;
  }

  const result = toResult({
    ...share,
    management_token: managementToken ?? null,
  });
  const folderName = deriveFolderName(files);
  return folderName ? { ...result, fileName: folderName } : result;
}

/** For a folder upload, show the folder's own name rather than one file inside it. */
function deriveFolderName(files: FileEntry[]): string | undefined {
  if (files.length < 2) {
    return;
  }
  const withRoot = files.find((entry) => entry.path.includes("/"));
  return withRoot?.path.split("/")[0];
}

function addFileToShare(
  code: string,
  file: File,
  filename: string,
  managementToken: string | undefined,
  options: UploadOptions
): Promise<ShareResponse> {
  return createShare(
    file,
    { filename },
    options,
    `/api/v1/shares/${encodeURIComponent(code)}/files`,
    managementToken
  );
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
  if ("expiresInHours" in settings) {
    // 0 小時在建立時代表「永不過期」,PATCH 這邊要用 null 表達。
    body.expires_in_hours =
      settings.expiresInHours === undefined || settings.expiresInHours === 0
        ? null
        : settings.expiresInHours;
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
    throw new Error(errorBody?.message ?? `Update failed: ${response.status}`);
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
function createShare(
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
      const errorBody = xhr.response as ErrorBody | null;
      reject(new Error(errorBody?.message ?? `Upload failed: ${xhr.status}`));
    });

    xhr.send(body);
  });
}
