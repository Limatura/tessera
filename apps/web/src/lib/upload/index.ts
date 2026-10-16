import { sealedSize } from "@tessera/crypto";

import type { Sealing } from "./encryption";
import {
  prepareSealing,
  sealedBlob,
  sealedDetailsFor,
  sealedNameFor,
  sealedSource,
  sealedText,
} from "./encryption";
import { uploadResumable } from "./resumable";
import { createShare, deleteShare, toResult, updateShare } from "./share-api";
import type {
  ShareQuery,
  ShareResponse,
  UploadOptions,
  UploadResultData,
} from "./share-api";

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
  /** The file name of a pasted file, stored as the share title. */
  title?: string;
}

export interface LinkUploadPayload {
  type: "link";
  url: string;
}

export type UploadPayload =
  | FileUploadPayload
  | PasteUploadPayload
  | LinkUploadPayload;

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
  // Files finished plus the current file's byte progress, as one overall percentage
  const reportProgress = (done: number, currentFilePercent: number) => {
    options.onProgress?.(((done + currentFilePercent / 100) / total) * 100);
  };

  const [first, ...rest] = files;
  if (!first) {
    throw new Error("No file selected");
  }

  // The folder name becomes the share title: the share page and the archive
  // download name both come from it, instead of one file inside the folder.
  const folderName = deriveFolderName(files);
  const title = folderName
    ? truncateChars(folderName, MAX_TITLE_CHARS)
    : undefined;
  const sealing = options.encryption
    ? await prepareSealing(options.encryption)
    : undefined;
  let share = await uploadOneFile(
    first.file,
    sealing
      ? {
          ...sealing.query,
          sealed_details: title
            ? sealedDetailsFor(sealing.key, { title })
            : undefined,
          sealed_name: sealedNameFor(
            sealing.key,
            first.file,
            first.path || first.file.name
          ),
        }
      : { filename: first.path || first.file.name, title },
    {
      onProgress: (percent) => reportProgress(0, percent),
      signal: options.signal,
    },
    sealing
  );
  // Returned only on creation, never on later add_file responses, so keep it for reuse
  const managementToken = share.management_token ?? undefined;
  reportProgress(1, 0);

  try {
    for (const [index, entry] of rest.entries()) {
      const done = index + 1;
      // Every file attaches to the same share, and progress must follow actual
      // completion order, so they cannot be sent in parallel.
      // oxlint-disable-next-line no-await-in-loop -- see the line above
      share = await addFileToShare(
        share.code,
        entry.file,
        entry.path || entry.file.name,
        managementToken,
        {
          onProgress: (percent) => reportProgress(done, percent),
          signal: options.signal,
        },
        sealing
      );
      reportProgress(done + 1, 0);
    }
    await presentAccessToken(share.code, sealing, managementToken);
  } catch (error) {
    await deleteShare(share.code, managementToken).catch(() => {
      // The original error is already being reported; a failed delete must not replace it
    });
    throw error;
  }

  const result = sealedResult(
    toResult({ ...share, management_token: managementToken ?? null }),
    sealing
  );
  return {
    ...result,
    fileName:
      folderName ?? (sealing ? first.path || first.file.name : result.fileName),
  };
}

/**
 * For a key from a password, sets the share's password to that key's access
 * token. `POST /shares` has nowhere to put it, so it follows creation; until
 * it lands the share is sealed but unlocked, which opens nothing without the
 * key, and a failure here deletes the share.
 */
async function presentAccessToken(
  code: string,
  sealing: Sealing | undefined,
  managementToken: string | undefined
) {
  if (sealing?.accessToken) {
    await updateShare(code, { password: sealing.accessToken }, managementToken);
  }
}

/** The link with its key, and whether the password can still change. */
function sealedResult(
  result: UploadResultData,
  sealing: Sealing | undefined
): UploadResultData {
  if (!sealing) {
    return result;
  }
  return {
    ...result,
    encrypted: true,
    hasPassword: sealing.accessToken !== undefined || result.hasPassword,
    passwordIsKey: sealing.accessToken !== undefined,
    shortUrl: sealing.fragment
      ? `${result.shortUrl}#${sealing.fragment}`
      : result.shortUrl,
  };
}

/** For a folder upload, show the folder's own name instead of one file inside it. */
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
  options: UploadOptions,
  sealing: Sealing | undefined
): Promise<ShareResponse> {
  // An added file is sealed the way its share is; `encryption` is not repeated
  return uploadOneFile(
    file,
    sealing
      ? { sealed_name: sealedNameFor(sealing.key, file, filename) }
      : { filename },
    options,
    sealing,
    `/api/v1/shares/${encodeURIComponent(code)}/files`,
    code,
    managementToken
  );
}

/**
 * Where a single file is routed. Over this instance's single-request limit
 * (`max_upload_bytes` from `GET /capabilities`, the same value the server uses
 * as its body size limit) it cannot go in one `POST`, so it uses a resumable
 * session instead (`POST /uploads`, then `PATCH` or direct parts to the store).
 *
 * Both paths return the same `ShareResponse`, so callers need not know which
 * one ran.
 *
 * `existingCode` attaches to an existing share via `POST /uploads?code=`;
 * leave it `undefined` for a new share. `basePath` is used only by the
 * single-`POST` path.
 */
async function uploadOneFile(
  file: File,
  query: ShareQuery,
  options: UploadOptions,
  sealing?: Sealing,
  basePath = "/api/v1/shares",
  existingCode?: string,
  managementToken?: string
): Promise<ShareResponse> {
  const maxUploadBytes = await fetchMaxUploadBytes(options.signal);
  // Sealed, a file is slightly larger, and that is what the limit counts
  const size = sealing ? sealedSize(file.size) : file.size;
  if (maxUploadBytes === undefined || size <= maxUploadBytes) {
    const body = sealing ? await sealedBlob(sealing.key, file) : file;
    return createShare(body, query, options, basePath, managementToken);
  }
  // `/uploads` takes no title, so a sealed one would be lost there
  const { sealed_details: _, ...sessionQuery } = query;
  return uploadResumable(
    sealing ? sealedSource(sealing.key, file) : file,
    sessionQuery,
    options,
    existingCode,
    managementToken
  );
}

/**
 * The limit, cached for the life of the page so a folder of hundreds of files
 * does not ask `/capabilities` for each. `undefined` means the lookup failed
 * (offline, or the instance has capabilities off); callers then use a single
 * `POST` as before. The server still refuses oversized requests by
 * `max_body_bytes`; only the routing to resumable before upload is lost.
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
    // Offline, or capabilities off: callers use a single POST as before
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
  if (options.encryption) {
    return uploadSealedPaste(payload, options);
  }
  return createShare(
    payload.content,
    {
      language: payload.language,
      title: payload.title
        ? truncateChars(payload.title, MAX_TITLE_CHARS)
        : undefined,
    },
    options
  ).then(toResult);
}

/**
 * A note sealed whole. Its title and language travel sealed too, so the share
 * page cannot even say which language it is until it has the key.
 */
async function uploadSealedPaste(
  payload: PasteUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  const sealing = await prepareSealing(
    options.encryption as NonNullable<UploadOptions["encryption"]>
  );
  const title = payload.title
    ? truncateChars(payload.title, MAX_TITLE_CHARS)
    : undefined;
  const share = await createShare(
    sealedText(sealing.key, payload.content),
    {
      ...sealing.query,
      sealed_details:
        title || payload.language
          ? sealedDetailsFor(sealing.key, {
              language: payload.language,
              title,
            })
          : undefined,
    },
    options
  );
  const managementToken = share.management_token ?? undefined;
  try {
    await presentAccessToken(share.code, sealing, managementToken);
  } catch (error) {
    await deleteShare(share.code, managementToken).catch(() => {
      // The original error is already being reported
    });
    throw error;
  }
  const result = sealedResult(toResult(share), sealing);
  return title ? { ...result, fileName: title } : result;
}

function uploadLink(
  payload: LinkUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  // No title: titles are capped at 191 characters and a URL can be up to 2048,
  // so a long one would be refused. The result screen already has the URL.
  return createShare(payload.url, { kind: "link" }, options).then((share) => ({
    ...toResult(share),
    fileName: payload.url,
  }));
}

/**
 * The server caps titles at 191 characters (MAX_TEXT_FIELD in apps/server);
 * over that the request is refused.
 */
const MAX_TITLE_CHARS = 191;

/** Truncates by code point, so a surrogate pair such as an emoji is never split. */
function truncateChars(text: string, max: number): string {
  return [...text].slice(0, max).join("");
}
