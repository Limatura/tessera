import type { components } from "@tessera/api/schema";

import { accepted, apiError, request, send } from "./context";
import type { Context, ErrorBody } from "./context";
import type { ShareQuery, ShareResponse, UploadOptions } from "./share-api";

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

/**
 * What a resumable upload reads its parts from. A `File` slices anywhere; a
 * sealed stream can only be read forward, so parts are asked for in order and
 * each exactly once (a part retried over `PATCH` reuses the bytes it got).
 */
export interface UploadSource {
  size: number;
  slice: (start: number, end: number) => Promise<Blob>;
}

function blobSource(blob: Blob): UploadSource {
  return {
    size: blob.size,
    slice: (start, end) => Promise.resolve(blob.slice(start, end)),
  };
}

/** Reads a stream forward into parts, holding only the part being sent. */
export function streamSource(
  stream: ReadableStream<Uint8Array>,
  size: number
): UploadSource {
  const reader = stream.getReader();
  let position = 0;
  let pending: Uint8Array[] = [];
  let buffered = 0;

  return {
    size,
    slice: async (start, end) => {
      if (start !== position) {
        throw new Error(`parts out of order: ${start} after ${position}`);
      }
      while (buffered < end - start) {
        // oxlint-disable-next-line no-await-in-loop -- a stream is read in order
        const { done, value } = await reader.read();
        if (done) {
          throw new Error(
            `the stream ended at ${position + buffered} of ${size}`
          );
        }
        pending.push(value);
        buffered += value.length;
      }
      const whole = new Blob(pending as BlobPart[]);
      const part = whole.slice(0, end - start);
      const rest = await whole.slice(end - start).arrayBuffer();
      pending = rest.byteLength > 0 ? [new Uint8Array(rest)] : [];
      buffered = rest.byteLength;
      position = end;
      return part;
    },
  };
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

/**
 * The I/O `runResumableUpload` needs, injected so the fallback/ordering logic
 * can be tested without real XHR/fetch.
 */
export interface ResumableIo {
  putPartDirect: OmitContext<typeof putPartDirect>;
  patchPart: OmitContext<typeof patchPart>;
  completeUpload: OmitContext<typeof completeUpload>;
}

type OmitContext<F> = F extends (context: Context, ...rest: infer A) => infer R
  ? (...args: A) => R
  : never;

function resumableIo(context: Context): ResumableIo {
  return {
    completeUpload: (...args) => completeUpload(context, ...args),
    patchPart: (...args) => patchPart(context, ...args),
    putPartDirect: (...args) => putPartDirect(context, ...args),
  };
}

/**
 * A file too big for one `POST` goes through a resumable session instead:
 * `POST /uploads` opens it, then each part either goes straight to the
 * store (`direct: true`) or through `PATCH` here, part by part in order
 * since the offset is the same session-wide state either way.
 *
 * A part `PUT` that fails falls back to `PATCH` for the rest of the upload
 * (see `docs/resumable.md`): a bucket with no CORS looks the same as a
 * `direct: true` upload the browser cannot actually reach.
 */
export async function uploadResumable(
  context: Context,
  file: Blob | UploadSource,
  query: ShareQuery,
  options: UploadOptions,
  existingCode?: string,
  managementToken?: string
): Promise<ShareResponse> {
  const session = await createUploadSession(
    context,
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
    resumableIo(context)
  );
}

/**
 * The pure orchestration: given an already-opened session, drives every
 * part to completion. Exported only so tests can inject fake I/O and
 * exercise the direct-PUT-fails-fall-back-to-PATCH path without real
 * XHR/fetch; see `resumable.test.ts`.
 */
export async function runResumableUpload(
  input: Blob | UploadSource,
  session: UploadSessionResponse,
  options: UploadOptions,
  managementToken: string | undefined,
  io: ResumableIo
): Promise<ShareResponse> {
  const file = input instanceof Blob ? blobSource(input) : input;
  const parts = planParts(file.size, session.part_size);
  const etags: string[] = [];
  // With direct, parts go straight to the store until a PUT fails, then PATCH.
  // It never switches back: PATCH appends bytes by offset, which depends on the
  // order of the parts already sent direct.
  let { direct } = session;
  const token = session.token ?? managementToken;

  for (const part of parts) {
    // oxlint-disable-next-line no-await-in-loop -- parts must be read in order
    const bytes = await file.slice(part.start, part.end);
    const reportPartProgress = (percent: number) => {
      const done = part.start + (bytes.size * percent) / 100;
      options.onProgress?.((done / file.size) * 100);
    };

    if (direct) {
      // oxlint-disable-next-line no-await-in-loop -- parts must be sent in order
      const etag = await io
        .putPartDirect(session.id, part.number, bytes, token, options.signal)
        .catch(() => null);
      if (etag) {
        etags.push(etag);
        reportPartProgress(100);
        continue;
      }
      // The PUT failed (a bucket without CORS, say): this part and the rest use PATCH.
      direct = false;
    }

    // oxlint-disable-next-line no-await-in-loop -- parts must be sent in order
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
  context: Context,
  file: Blob | UploadSource,
  query: ShareQuery,
  existingCode: string | undefined,
  managementToken: string | undefined,
  signal: AbortSignal | undefined
): Promise<UploadSessionResponse> {
  const params = new URLSearchParams();
  // The only parameters `/uploads` takes; the rest of a share's details go on
  // `POST /shares` alone.
  for (const key of [
    "filename",
    "encryption",
    "key_salt",
    "sealed_name",
  ] as const) {
    const value = query[key];
    if (value !== undefined) {
      params.set(key, value);
    }
  }
  if (existingCode) {
    params.set("code", existingCode);
  }
  const search = params.toString();

  const headers: Record<string, string> = {
    "Upload-Length": String(file.size),
  };
  if (managementToken) {
    headers["X-Upload-Token"] = managementToken;
  }

  const response = await accepted(
    await request(context, `/api/v1/uploads${search ? `?${search}` : ""}`, {
      headers,
      method: "POST",
      signal,
    }),
    "Upload session failed"
  );
  return response.json() as Promise<UploadSessionResponse>;
}

/** `GET` the part's direct URL, `PUT` the bytes, and return the store's `ETag`. */
async function putPartDirect(
  context: Context,
  sessionId: string,
  partNumber: number,
  bytes: Blob,
  token: string | null | undefined,
  signal: AbortSignal | undefined
): Promise<string> {
  const headers: Record<string, string> = {};
  if (token) {
    headers["X-Upload-Token"] = token;
  }
  const urlResponse = await request(
    context,
    `/api/v1/uploads/${encodeURIComponent(sessionId)}/parts/${partNumber}`,
    { headers, signal }
  );
  if (!urlResponse.ok) {
    throw new Error(`Part URL request failed: ${urlResponse.status}`);
  }
  const { url } = (await urlResponse.json()) as PartUrlResponse;

  // Straight to the store: neither the instance's credentials nor its headers.
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
async function patchPart(
  context: Context,
  sessionId: string,
  offset: number,
  bytes: Blob,
  token: string | null | undefined,
  options: UploadOptions
): Promise<PatchOutcome> {
  const headers: Record<string, string> = {
    "Content-Type": "application/offset+octet-stream",
    "Upload-Offset": String(offset),
  };
  if (token) {
    headers["X-Upload-Token"] = token;
  }
  const response = await send(context, {
    body: bytes,
    headers,
    method: "PATCH",
    onProgress: options.onProgress,
    path: `/api/v1/uploads/${encodeURIComponent(sessionId)}`,
    signal: options.signal,
  });
  if (response.status === 201) {
    return { done: true, share: response.body as ShareResponse };
  }
  if (response.status === 204) {
    return { done: false };
  }
  throw apiError(
    response.body as ErrorBody | null,
    `Upload failed: ${response.status}`,
    response.status
  );
}

async function completeUpload(
  context: Context,
  sessionId: string,
  etags: string[],
  token: string | null | undefined,
  signal: AbortSignal | undefined
): Promise<ShareResponse> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (token) {
    headers["X-Upload-Token"] = token;
  }
  const response = await accepted(
    await request(
      context,
      `/api/v1/uploads/${encodeURIComponent(sessionId)}/complete`,
      { body: JSON.stringify({ etags }), headers, method: "POST", signal }
    ),
    "Upload completion failed"
  );
  return response.json() as Promise<ShareResponse>;
}
