import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * Shares uploaded anonymously from this browser. The management token returned
 * at creation is the only credential for changing or deleting them, and it used
 * to live only in the result screen's memory, lost when the tab closed. Stored
 * here, the home page can list recent uploads and still change expiry or
 * password, or delete them.
 *
 * The tokens are stored as is, so this list is the key to those shares: other
 * users of the same computer, or XSS on this domain, can read it. But a token
 * can only change or delete its share, not read the content (a password is still
 * needed) or touch an account, so the convenience is worth it. Entries can be
 * removed one by one.
 *
 * Shares uploaded while signed in are not kept here; they belong to the account
 * and appear under My shares.
 */
export interface RecentShare {
  code: string;
  managementToken: string;
  fileName: string;
  shortUrl: string;
  createdAt: string;
  /**
   * The last known settings. `GET /shares/{code}` omits expiry and download
   * count for a password-protected share, so the list can only show this
   * snapshot; opening the manager prefers the server's values when it can read
   * them.
   */
  expiresAt: string | null;
  usesRemaining: number | null;
  hasPassword: boolean;
  /**
   * Encrypted with a key from a password, so its password cannot change.
   * For a key in the link, `shortUrl` carries the key after `#`: without it
   * the share could not be opened again from here.
   */
  passwordIsKey?: boolean;
}

/**
 * Past this the oldest is dropped. The list is for "the one I just uploaded",
 * not a full history.
 */
const MAX_ENTRIES = 20;

interface RecentSharesState {
  shares: RecentShare[];
  remember: (share: RecentShare) => void;
  update: (code: string, patch: Partial<RecentShare>) => void;
  forget: (code: string) => void;
}

export const useRecentSharesStore = create<RecentSharesState>()(
  persist(
    (set) => ({
      forget: (code) =>
        set((state) => ({
          shares: state.shares.filter((share) => share.code !== code),
        })),
      remember: (share) =>
        set((state) => ({
          shares: [
            share,
            ...state.shares.filter((item) => item.code !== share.code),
          ].slice(0, MAX_ENTRIES),
        })),
      shares: [],
      update: (code, patch) =>
        set((state) => ({
          shares: state.shares.map((share) =>
            share.code === code ? { ...share, ...patch } : share
          ),
        })),
    }),
    { name: "tessera-recent-shares", version: 1 }
  )
);
