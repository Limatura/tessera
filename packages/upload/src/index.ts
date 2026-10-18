/**
 * Making and managing shares, for any client of a Tessera instance: the web
 * app, served by the instance itself, and the browser extension, which talks
 * to whichever instance its user configured.
 *
 * No framework here: everything is a function returning a `Promise`, which a
 * caller wraps in `useMutation`, a service worker's message handler, or
 * nothing at all.
 */
import { toContext } from "./context";
import type { UploaderConfig } from "./context";
import { deleteShare, toResult, updateShare } from "./share-api";
import type {
  ShareResponse,
  ShareSettings,
  UploadOptions,
  UploadResultData,
} from "./share-api";
import { upload } from "./upload";
import type { UploadPayload } from "./upload";

export { apiError } from "./context";
export type { ErrorBody, UploaderConfig } from "./context";
export type { EncryptionChoice } from "./encryption";
export type {
  ShareResponse,
  ShareSettings,
  UploadOptions,
  UploadResultData,
} from "./share-api";
export type {
  FileEntry,
  FileUploadPayload,
  LinkUploadPayload,
  PasteUploadPayload,
  UploadPayload,
} from "./upload";
export { minutesUntil } from "./share-api";

/** Every operation, bound to one instance. */
export interface Uploader {
  /** The instance's origin, normalised. */
  readonly baseUrl: string;
  /**
   * Makes a share. A multi-file upload creates the share with the first file
   * and attaches the rest one by one; if any later file fails, the half-built
   * share is deleted. A file over `max_upload_bytes` goes resumable.
   */
  upload: (
    payload: UploadPayload,
    options?: UploadOptions
  ) => Promise<UploadResultData>;
  /** Changes a share after it exists. See `updateShare` in `share-api.ts`. */
  updateShare: (
    code: string,
    settings: ShareSettings,
    managementToken?: string
  ) => Promise<void>;
  deleteShare: (code: string, managementToken?: string) => Promise<void>;
  /** A share as the result screen shows it, with its link on this instance. */
  toResult: (share: ShareResponse) => UploadResultData;
}

export function createUploader(config: UploaderConfig): Uploader {
  const context = toContext(config);
  return {
    baseUrl: context.baseUrl,
    deleteShare: (code, managementToken) =>
      deleteShare(context, code, managementToken),
    toResult: (share) => toResult(context, share),
    updateShare: (code, settings, managementToken) =>
      updateShare(context, code, settings, managementToken),
    upload: (payload, options) => upload(context, payload, options),
  };
}
