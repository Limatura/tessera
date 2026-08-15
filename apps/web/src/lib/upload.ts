import type { components } from "@tessera/api/schema";

export interface UploadResultData {
  fileName: string;
  shortUrl: string;
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
    fileName: share.filename ?? share.title ?? share.code,
    shortUrl: new URL(share.path, window.location.origin).toString(),
  };
}

interface ShareQuery {
  filename?: string;
  title?: string;
  language?: string;
  expires_in_hours?: number;
  max_uses?: number;
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
