import type { components } from "@tessera/api/schema";

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

export type ShareResponse = components["schemas"]["ShareResponse"];
type PatchShareRequest = components["schemas"]["PatchShareRequest"];
type ErrorBody = components["schemas"]["ErrorBody"];
export type UploadSessionResponse = components["schemas"]["UploadResponse"];
type PartUrlResponse = components["schemas"]["PartUrlResponse"];

/** 失敗時丟的 `Error`；掛上 `code`、`details` 讓畫面能用 `apiErrorMessage` 翻譯。 */
function apiError(body: ErrorBody | null, fallback: string): Error {
  const error = new Error(body?.message ?? fallback);
  return body
    ? Object.assign(error, { code: body.code, details: body.details })
    : error;
}

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
  // 完成的檔案數 + 目前檔案的位元組進度，合成整體百分比
  const reportProgress = (done: number, currentFilePercent: number) => {
    options.onProgress?.(((done + currentFilePercent / 100) / total) * 100);
  };

  const [first, ...rest] = files;
  if (!first) {
    throw new Error("No file selected");
  }

  let share = await uploadOneFile(
    first.file,
    { filename: first.path || first.file.name },
    {
      onProgress: (percent) => reportProgress(0, percent),
      signal: options.signal,
    }
  );
  // 只有建立時回傳，之後每個 add_file 的回應都不會再帶，記下來重複使用
  const managementToken = share.management_token ?? undefined;
  reportProgress(1, 0);

  try {
    for (const [index, entry] of rest.entries()) {
      const done = index + 1;
      // oxlint-disable-next-line no-await-in-loop -- 要掛到同一個 share，且進度要照完成順序回報，不能平行送
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
      // 已回報原本的錯誤，刪除失敗不再蓋過去
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
  return uploadOneFile(
    file,
    { filename },
    options,
    `/api/v1/shares/${encodeURIComponent(code)}/files`,
    code,
    managementToken
  );
}

/**
 * 單一檔案的分派點:超過 instance 的單次上傳上限(`GET /capabilities` 的
 * `max_upload_bytes`,後端也用它當 body size limit)就無法一次 `POST` 送完,
 * 改走 resumable session(`POST /uploads` + `PATCH`/直傳 S3)。兩條路徑回傳
 * 同一個 `ShareResponse`,呼叫端不用分辨走了哪一條。
 *
 * `existingCode`:掛到既有 share 時傳給 `POST /uploads?code=`,建新 share 留
 * `undefined`。`basePath` 只有單次 `POST` 會用到。
 */
async function uploadOneFile(
  file: File,
  query: ShareQuery,
  options: UploadOptions,
  basePath = "/api/v1/shares",
  existingCode?: string,
  managementToken?: string
): Promise<ShareResponse> {
  const maxUploadBytes = await fetchMaxUploadBytes(options.signal);
  if (maxUploadBytes === undefined || file.size <= maxUploadBytes) {
    return createShare(file, query, options, basePath, managementToken);
  }
  return uploadResumable(file, query, options, existingCode, managementToken);
}

/**
 * 快取頁面存活期間查到的上限,一個資料夾幾百個檔案不用每個都打一次
 * `/capabilities`。`undefined` 代表查詢失敗(離線、instance 沒開 capabilities),
 * 讓呼叫端照舊走單次 `POST`——後端本來就會依 `max_body_bytes` 擋下過大請求,
 * 只是少了「上傳前先分流到 resumable」這個優化。
 */
let cachedMaxUploadBytes: Promise<number | undefined> | undefined;

async function requestMaxUploadBytes(
  signal: AbortSignal | undefined
): Promise<number | undefined> {
  try {
    const response = await fetch("/api/v1/capabilities", {
      credentials: "include",
      signal,
    });
    if (!response.ok) {
      return;
    }
    const body = (await response.json()) as { max_upload_bytes?: number };
    return body.max_upload_bytes;
  } catch {
    // 離線或 instance 沒開 capabilities:照舊走單次 POST
  }
}

function fetchMaxUploadBytes(
  signal: AbortSignal | undefined
): Promise<number | undefined> {
  cachedMaxUploadBytes ??= requestMaxUploadBytes(signal);
  return cachedMaxUploadBytes;
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
  // 不帶 title:上限 191 bytes,網址可到 2048,塞長網址會被拒;顯示用的網址直接拿手上這個就好。
  return createShare(payload.url, { kind: "link" }, options).then((share) => ({
    ...toResult(share),
    fileName: payload.url,
  }));
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

interface ShareQuery {
  /** 只有連結分享要帶;檔案跟文字由其他參數區分。 */
  kind?: "link";
  filename?: string;
  title?: string;
  language?: string;
  expires_in_hours?: number;
  max_uses?: number;
}

/** `DELETE /api/v1/shares/{code}` 需要登入 session,或匿名上傳拿到的 `management_token`(走 `X-Share-Management-Token` header)。 */
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
 * 授權方式跟 DELETE 一樣:登入 session 或匿名的 `management_token`。
 *
 * 三個欄位都是「省略 = 不動」,跟「送 null = 清掉」不一樣,所以只把呼叫端
 * 真的指定的欄位放進 body。
 *
 * 密碼是特例:設密碼走 `X-Share-Password` header(不進 query string、access
 * log、Referer),移除密碼才走 body 的 `remove_password`——後端會拒絕兩個一起送,
 * 所以這裡二選一。
 */
export async function updateShare(
  code: string,
  settings: ShareSettings,
  managementToken?: string
): Promise<void> {
  const body: PatchShareRequest = {};
  if ("expiresInMinutes" in settings) {
    // 0 在建立時代表「永不過期」,PATCH 這邊用 null 表達同一件事。
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

/** `fetch` 沒有上傳進度事件，改用 `XMLHttpRequest`，因此需要這個本可避免的 `Promise` wrapper。 */
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

/** One entry in a `Upload-Length`-declared session's part plan. */
interface UploadPart {
  /** From 1, per the contract. */
  number: number;
  /** Byte offset into the file where this part starts. */
  start: number;
  /** Exclusive end offset. */
  end: number;
}

/** Splits a file into `part_size`-sized parts; the last part takes the remainder. */
function planParts(totalBytes: number, partSize: number): UploadPart[] {
  const parts: UploadPart[] = [];
  let start = 0;
  let number = 1;
  while (start < totalBytes) {
    const end = Math.min(start + partSize, totalBytes);
    parts.push({ end, number, start });
    start = end;
    number += 1;
  }
  return parts;
}

/** The I/O `runResumableUpload` needs, injected so the fallback/ordering logic can be tested without real XHR/fetch. */
export interface ResumableIo {
  putPartDirect: typeof putPartDirect;
  patchPart: typeof patchPart;
  completeUpload: typeof completeUpload;
}

const resumableIo: ResumableIo = { completeUpload, patchPart, putPartDirect };

/**
 * A file too big for one `POST` goes through a resumable session instead:
 * `POST /uploads` opens it, then each part goes straight to the store
 * (`direct: true`) or through `PATCH`, in order since the offset is
 * session-wide state either way.
 *
 * A failed part `PUT` falls back to `PATCH` for the rest of the upload —
 * see `docs/resumable.md`: a bucket with no CORS looks exactly like a
 * `direct: true` upload the browser can't actually reach.
 */
async function uploadResumable(
  file: File,
  query: ShareQuery,
  options: UploadOptions,
  existingCode?: string,
  managementToken?: string
): Promise<ShareResponse> {
  const session = await createUploadSession(
    file,
    query,
    existingCode,
    managementToken,
    options.signal
  );
  return runResumableUpload(
    file,
    session,
    options,
    managementToken,
    resumableIo
  );
}

/**
 * The pure orchestration: drives an already-opened session's parts to
 * completion. Exported only so tests can inject fake I/O and exercise the
 * direct-PUT-fails-falls-back-to-PATCH path without real XHR/fetch — see
 * `upload.test.ts`.
 */
export async function runResumableUpload(
  file: File,
  session: UploadSessionResponse,
  options: UploadOptions,
  managementToken: string | undefined,
  io: ResumableIo
): Promise<ShareResponse> {
  const parts = planParts(file.size, session.part_size);
  const etags: string[] = [];
  // 直傳失敗才轉走 PATCH，轉了就不切回去——PATCH 是照 offset 累加位元組，
  // 跟已經直傳出去的片段順序相關。
  let { direct } = session;
  const token = session.token ?? managementToken;

  for (const part of parts) {
    const bytes = file.slice(part.start, part.end);
    const reportPartProgress = (percent: number) => {
      const done = part.start + (bytes.size * percent) / 100;
      options.onProgress?.((done / file.size) * 100);
    };

    if (direct) {
      // oxlint-disable-next-line no-await-in-loop -- 分片必須按順序送出
      const etag = await io
        .putPartDirect(session.id, part.number, bytes, token, options.signal)
        .catch(() => null);
      if (etag) {
        etags.push(etag);
        reportPartProgress(100);
        continue;
      }
      // PUT 失敗（例如 bucket 沒設 CORS）：這片起改走 PATCH。
      direct = false;
    }

    // oxlint-disable-next-line no-await-in-loop -- 分片必須按順序送出
    const outcome = await io.patchPart(session.id, part.start, bytes, token, {
      onProgress: reportPartProgress,
      signal: options.signal,
    });
    if (outcome.done) {
      return outcome.share;
    }
  }

  return io.completeUpload(session.id, etags, token, options.signal);
}

async function createUploadSession(
  file: File,
  query: ShareQuery,
  existingCode: string | undefined,
  managementToken: string | undefined,
  signal: AbortSignal | undefined
): Promise<UploadSessionResponse> {
  const params = new URLSearchParams();
  if (query.filename) {
    params.set("filename", query.filename);
  }
  if (existingCode) {
    params.set("code", existingCode);
  }
  const search = params.toString();

  const headers: HeadersInit = { "Upload-Length": String(file.size) };
  if (managementToken) {
    headers["X-Upload-Token"] = managementToken;
  }

  const response = await fetch(`/api/v1/uploads${search ? `?${search}` : ""}`, {
    credentials: "include",
    headers,
    method: "POST",
    signal,
  });
  if (!response.ok) {
    const errorBody = (await response
      .json()
      .catch(() => null)) as ErrorBody | null;
    throw apiError(errorBody, `Upload session failed: ${response.status}`);
  }
  return response.json() as Promise<UploadSessionResponse>;
}

/** `GET` the part's direct URL, `PUT` the bytes, and return the store's `ETag`. */
async function putPartDirect(
  sessionId: string,
  partNumber: number,
  bytes: Blob,
  token: string | null | undefined,
  signal: AbortSignal | undefined
): Promise<string> {
  const headers: HeadersInit = {};
  if (token) {
    headers["X-Upload-Token"] = token;
  }
  const urlResponse = await fetch(
    `/api/v1/uploads/${encodeURIComponent(sessionId)}/parts/${partNumber}`,
    { credentials: "include", headers, signal }
  );
  if (!urlResponse.ok) {
    throw new Error(`Part URL request failed: ${urlResponse.status}`);
  }
  const { url } = (await urlResponse.json()) as PartUrlResponse;

  const putResponse = await fetch(url, { body: bytes, method: "PUT", signal });
  if (!putResponse.ok) {
    throw new Error(`Direct part upload failed: ${putResponse.status}`);
  }
  const etag = putResponse.headers.get("ETag");
  if (!etag) {
    throw new Error("Store did not answer with an ETag");
  }
  return etag;
}

export type PatchOutcome =
  | { done: false }
  | { done: true; share: ShareResponse };

/** `PATCH`es one part at `offset`. `204` keeps going; `201` means this was the last part and the session is a share now. */
function patchPart(
  sessionId: string,
  offset: number,
  bytes: Blob,
  token: string | null | undefined,
  options: UploadOptions
): Promise<PatchOutcome> {
  // oxlint-disable-next-line promise/avoid-new -- XHR 的進度事件只能透過回呼取得
  return new Promise((resolve, reject) => {
    if (options.signal?.aborted) {
      reject(
        options.signal.reason ?? new DOMException("Aborted", "AbortError")
      );
      return;
    }

    const xhr = new XMLHttpRequest();
    xhr.open("PATCH", `/api/v1/uploads/${encodeURIComponent(sessionId)}`);
    xhr.responseType = "json";
    xhr.withCredentials = true;
    xhr.setRequestHeader("Content-Type", "application/offset+octet-stream");
    xhr.setRequestHeader("Upload-Offset", String(offset));
    if (token) {
      xhr.setRequestHeader("X-Upload-Token", token);
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
      if (xhr.status === 201) {
        resolve({ done: true, share: xhr.response as ShareResponse });
        return;
      }
      if (xhr.status === 204) {
        resolve({ done: false });
        return;
      }
      reject(
        apiError(
          xhr.response as ErrorBody | null,
          `Upload failed: ${xhr.status}`
        )
      );
    });

    xhr.send(bytes);
  });
}

async function completeUpload(
  sessionId: string,
  etags: string[],
  token: string | null | undefined,
  signal: AbortSignal | undefined
): Promise<ShareResponse> {
  const headers: HeadersInit = { "Content-Type": "application/json" };
  if (token) {
    headers["X-Upload-Token"] = token;
  }
  const response = await fetch(
    `/api/v1/uploads/${encodeURIComponent(sessionId)}/complete`,
    {
      body: JSON.stringify({ etags }),
      credentials: "include",
      headers,
      method: "POST",
      signal,
    }
  );
  if (!response.ok) {
    const errorBody = (await response
      .json()
      .catch(() => null)) as ErrorBody | null;
    throw apiError(errorBody, `Upload completion failed: ${response.status}`);
  }
  return response.json() as Promise<ShareResponse>;
}
