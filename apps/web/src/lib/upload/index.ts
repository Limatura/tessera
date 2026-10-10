import { uploadResumable } from "./resumable";
import { createShare, deleteShare, toResult } from "./share-api";
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
  /** 貼上檔案時的檔名,存成分享標題。 */
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
  // 已完成的檔案數加上目前這個檔案的位元組進度，合成成整體百分比
  const reportProgress = (done: number, currentFilePercent: number) => {
    options.onProgress?.(((done + currentFilePercent / 100) / total) * 100);
  };

  const [first, ...rest] = files;
  if (!first) {
    throw new Error("No file selected");
  }

  // 資料夾的名字存成分享標題:分享頁與壓縮檔下載名都從這裡來,
  // 不然會顯示資料夾裡的某一個檔案。
  const folderName = deriveFolderName(files);
  let share = await uploadOneFile(
    first.file,
    {
      filename: first.path || first.file.name,
      title: folderName
        ? truncateChars(folderName, MAX_TITLE_CHARS)
        : undefined,
    },
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
 * 單一檔案的分派點:超過這個 instance 的單次上傳上限(`GET /capabilities`
 * 的 `max_upload_bytes`,後端也用同一個值當 body size limit)就無法用一次
 * `POST` 送完,改走 resumable session(`POST /uploads` + `PATCH`/直傳 S3)。
 *
 * 兩條路徑回傳同一個 `ShareResponse`,呼叫端不用分辨走了哪一條。
 *
 * `existingCode`:掛到既有 share 時要傳給 `POST /uploads?code=`,
 * 建新 share 就留 `undefined`。`basePath` 只有單次 `POST` 這條路會用到。
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
 * 快取住這次頁面存活期間查到的上限,一個資料夾裡幾百個檔案不用每個都打一次
 * `/capabilities`。`undefined` 代表查詢失敗(離線、instance 沒開
 * capabilities 之類),此時讓呼叫端照舊走單次 `POST`——後端本來就會依
 * `max_body_bytes` 擋下太大的請求,只是少了「上傳前就先分流到 resumable」
 * 這個優化。
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
    // 離線或 instance 沒開 capabilities:讓呼叫端照舊走單次 POST
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
    {
      language: payload.language,
      title: payload.title
        ? truncateChars(payload.title, MAX_TITLE_CHARS)
        : undefined,
    },
    options
  ).then(toResult);
}

function uploadLink(
  payload: LinkUploadPayload,
  options: UploadOptions
): Promise<UploadResultData> {
  // 不帶 title:title 上限 191 bytes,網址最長可到 2048,塞進去長網址會被拒。
  // 結果畫面要顯示的網址就在手上,直接用。
  return createShare(payload.url, { kind: "link" }, options).then((share) => ({
    ...toResult(share),
    fileName: payload.url,
  }));
}

/** 後端 title 上限 191 個字元(apps/server 的 MAX_TEXT_FIELD,按字元數算),超過整個請求會被拒。 */
const MAX_TITLE_CHARS = 191;

/** 照 code point 截斷,不切開 emoji 這類代理對。 */
function truncateChars(text: string, max: number): string {
  return [...text].slice(0, max).join("");
}
