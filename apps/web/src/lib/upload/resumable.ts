import type { components } from "@tessera/api/schema";

import { apiError } from "./share-api";
import type {
  ErrorBody,
  ShareQuery,
  ShareResponse,
  UploadOptions,
} from "./share-api";

export type UploadSessionResponse = components["schemas"]["UploadResponse"];
type PartUrlResponse = components["schemas"]["PartUrlResponse"];

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
 * `POST /uploads` opens it, then each part either goes straight to the
 * store (`direct: true`) or through `PATCH` here, part by part in order
 * since the offset is the same session-wide state either way.
 *
 * A part `PUT` that fails falls back to `PATCH` for the rest of the upload
 * — see `docs/resumable.md`: a bucket with no CORS looks exactly like a
 * `direct: true` upload the browser cannot actually reach.
 */
export async function uploadResumable(
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
 * The pure orchestration: given an already-opened session, drives every
 * part to completion. Exported (only) so tests can inject fake I/O and
 * exercise the direct-PUT-fails-fall-back-to-PATCH path without real
 * XHR/fetch — see `resumable.test.ts`.
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
  // 走 direct 的話一路直傳失敗才會轉走 PATCH；一旦轉了就不會再切回去，
  // 因為 PATCH 是照 offset 累加位元組，跟已經直傳出去的那些片段順序相關。
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
      // PUT 失敗（例如 bucket 沒設 CORS）：這一片改走 PATCH，之後的片也是。
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

/**
 * `PATCH`es one part at `offset`. `204` means keep going; `201` means this
 * was the last part and the session is a share now.
 */
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
