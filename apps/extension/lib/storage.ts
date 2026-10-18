/**
 * Everything the extension keeps, and where.
 *
 * - `local:` stays on this browser. The instance's address goes here rather
 *   than in `sync:` because the permission to reach it is granted per install,
 *   and a synced address with no permission behind it would just fail.
 * - `sync:` follows the user's browser account: only preferences.
 * - `session:` lives until the browser closes: what is uploading right now
 *   (see `jobs.ts`, which keeps one key per upload).
 */
import { storage } from "#imports";

/** An account's API token, minted by the extension or pasted in by hand. */
export interface Credential {
  /** The instance it was issued by. Never sent anywhere else. */
  server: string;
  /** The `Authorization: Bearer` secret. */
  token: string;
  /** For revoking it on sign-out; `null` for a pasted token, whose id is unknown. */
  tokenId: string | null;
  username: string;
}

export interface UploadDefaults {
  /**
   * Seal uploads in the browser, the key in the link after `#`. Only applied
   * while the instance allows encryption; the setting is hidden otherwise.
   */
  encrypt: boolean;
  /** `null` leaves it to the instance's default; `0` never expires. */
  expiresInHours: number | null;
  /** `null` for no limit; `1` is burn-after-reading. */
  maxUses: number | null;
  /** Put the link on the clipboard when an upload finishes. */
  copyLink: boolean;
  /** Say so with a system notification when an upload finishes or fails. */
  notify: boolean;
}

/** A share this extension made, as much as is needed to come back to it. */
export interface KeptShare {
  server: string;
  code: string;
  /** The whole link, with the key after `#` for a share sealed with one. */
  link: string;
  /** What the user knows it by. A sealed share's name is not on the server in the clear. */
  name: string;
  kind: "file" | "note" | "link";
  encrypted: boolean;
  /** Only an anonymous upload has one; it is the only way to change or delete it. */
  managementToken?: string;
  createdAt: number;
}

export type JobState =
  | "preparing"
  | "uploading"
  | "done"
  | "failed"
  | "cancelled";

/** One upload, from the moment it is asked for until it is cleared. */
export interface Job {
  id: string;
  name: string;
  kind: "file" | "note" | "link";
  state: JobState;
  /** 0 to 100. */
  progress: number;
  startedAt: number;
  link?: string;
  error?: string;
}

export const serverItem = storage.defineItem<string | null>("local:server", {
  fallback: null,
});

export const credentialItem = storage.defineItem<Credential | null>(
  "local:credential",
  { fallback: null }
);

export const DEFAULT_UPLOAD_DEFAULTS: UploadDefaults = {
  copyLink: true,
  encrypt: false,
  expiresInHours: null,
  maxUses: null,
  notify: true,
};

export const defaultsItem = storage.defineItem<UploadDefaults>(
  "sync:defaults",
  { fallback: DEFAULT_UPLOAD_DEFAULTS }
);

/** Newest first, capped at {@link MAX_KEPT_SHARES}. */
export const sharesItem = storage.defineItem<KeptShare[]>("local:shares", {
  fallback: [],
});

export const MAX_KEPT_SHARES = 100;

/** The credential, but only when it belongs to the instance configured now. */
export async function currentCredential(): Promise<Credential | null> {
  const [server, credential] = await Promise.all([
    serverItem.getValue(),
    credentialItem.getValue(),
  ]);
  return credential && credential.server === server ? credential : null;
}

export async function keepShare(share: KeptShare): Promise<void> {
  const shares = await sharesItem.getValue();
  const rest = shares.filter(
    (kept) => !(kept.server === share.server && kept.code === share.code)
  );
  await sharesItem.setValue([share, ...rest].slice(0, MAX_KEPT_SHARES));
}

export async function forgetShare(server: string, code: string): Promise<void> {
  const shares = await sharesItem.getValue();
  await sharesItem.setValue(
    shares.filter((kept) => !(kept.server === server && kept.code === code))
  );
}
