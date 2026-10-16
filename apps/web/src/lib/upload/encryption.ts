import type { ShareKey } from "@tessera/crypto";
import {
  keyFromPassword,
  newKey,
  newSalt,
  ready,
  sealBytes,
  sealDetails,
  sealName,
  sealStream,
  sealedSize,
} from "@tessera/crypto";

import type { UploadSource } from "./resumable";
import { streamSource } from "./resumable";
import type { ShareQuery } from "./share-api";

/**
 * How an end-to-end encrypted share holds its key. See `docs/encryption.md`.
 *
 * - `link`: random, written after `#` in the link, which a browser never sends.
 * - `password`: derived from a password the server never sees.
 */
export type EncryptionChoice =
  | { mode: "link" }
  | { mode: "password"; password: string };

/** A share's key and what each request about it has to carry. */
export interface Sealing {
  key: ShareKey;
  /** For the request that creates the share. */
  query: Pick<ShareQuery, "encryption" | "key_salt">;
  /** What goes after `#` in the link, or nothing when the key is a password's. */
  fragment?: string;
  /**
   * The share password, for a key from a password: the server unlocks with
   * this token, never the password itself.
   */
  accessToken?: string;
}

/**
 * Makes the key. Deriving one from a password takes a fraction of a second
 * on a desktop and more on a phone, and blocks while it runs, so this yields a
 * frame first: whatever busy state the caller just set gets painted.
 */
export async function prepareSealing(
  choice: EncryptionChoice
): Promise<Sealing> {
  await ready();
  if (choice.mode === "link") {
    const key = newKey();
    return { fragment: key.encode(), key, query: { encryption: "link" } };
  }
  await nextFrame();
  const salt = newSalt();
  const key = keyFromPassword(choice.password, salt);
  return {
    accessToken: key.accessToken(),
    key,
    query: { encryption: "password", key_salt: salt },
  };
}

export function sealedNameFor(key: ShareKey, file: File, path: string) {
  return sealName(key, { name: path, type: file.type || undefined });
}

export function sealedDetailsFor(
  key: ShareKey,
  details: { title?: string; language?: string }
) {
  return sealDetails(key, details);
}

/** A file sealed as it is read, for a resumable upload. */
export function sealedSource(key: ShareKey, file: File): UploadSource {
  return streamSource(
    file.stream().pipeThrough(sealStream(key)),
    sealedSize(file.size)
  );
}

/**
 * A file sealed whole, for a single `POST`, which `XMLHttpRequest` cannot
 * send as a stream. Only ever below `max_upload_bytes`.
 */
export function sealedBlob(key: ShareKey, file: File): Promise<Blob> {
  return new Response(file.stream().pipeThrough(sealStream(key))).blob();
}

export function sealedText(key: ShareKey, text: string): Blob {
  return new Blob([sealBytes(key, new TextEncoder().encode(text)) as BlobPart]);
}

function nextFrame() {
  // oxlint-disable-next-line promise/avoid-new -- requestAnimationFrame only takes a callback
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => setTimeout(resolve, 0));
  });
}
