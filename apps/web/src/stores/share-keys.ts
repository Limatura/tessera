import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * The keys of link-encrypted shares uploaded while signed in, by code.
 *
 * The key lives only after `#` in the link and the server never has it, so
 * once the result screen closes nothing else could rebuild a link that opens
 * the share. Anonymous uploads keep the whole link in `recent-shares`; account
 * uploads are listed from the server under My shares, which merges these back
 * in. Like that store, this one is readable by anyone using this browser.
 */
interface ShareKeysState {
  keys: Record<string, string>;
  remember: (code: string, key: string) => void;
  forget: (code: string) => void;
}

export const useShareKeysStore = create<ShareKeysState>()(
  persist(
    (set) => ({
      forget: (code) =>
        set((state) => {
          const { [code]: _, ...keys } = state.keys;
          return { keys };
        }),
      keys: {},
      remember: (code, key) =>
        set((state) => ({ keys: { ...state.keys, [code]: key } })),
    }),
    { name: "tessera-share-keys", version: 1 }
  )
);
