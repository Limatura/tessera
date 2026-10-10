import { create } from "zustand";

export interface NativeShareData {
  title: string;
  url: string;
}

type NativeShareResult =
  | { status: "shared" }
  | { status: "cancelled" }
  | { error: unknown; status: "failed" }
  | { status: "unsupported" };

interface ShareState {
  canShare: boolean;
  share: (data: NativeShareData) => Promise<NativeShareResult>;
}

export const useShareStore = create<ShareState>()(() => ({
  canShare: typeof navigator !== "undefined" && "share" in navigator,
  share: async (data) => {
    if (typeof navigator === "undefined" || !navigator.share) {
      return { status: "unsupported" };
    }

    try {
      await navigator.share(data);
      return { status: "shared" };
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return { status: "cancelled" };
      }
      return { error, status: "failed" };
    }
  },
}));
