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
  _payload: FileUploadPayload,
  _options: UploadOptions
): Promise<UploadResultData> {
  throw new Error("uploadFile not implemented");
}

function uploadPaste(
  _payload: PasteUploadPayload,
  _options: UploadOptions
): Promise<UploadResultData> {
  throw new Error("uploadPaste not implemented");
}

function uploadLink(
  _payload: LinkUploadPayload,
  _options: UploadOptions
): Promise<UploadResultData> {
  throw new Error("uploadLink not implemented");
}
