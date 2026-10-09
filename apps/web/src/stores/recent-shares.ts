import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * 這台瀏覽器匿名上傳過的分享（#364）。匿名上傳要改設定、刪除，唯一的憑證是
 * 建立時拿到的管理 token —— 以前只活在結果畫面的記憶體裡，關掉分頁就再也改
 * 不了。存在這裡，首頁才列得出「最近上傳的」，點進去還能改期限、密碼或刪掉。
 *
 * 存的是 token 本身，這份清單等於那些分享的鑰匙：同一台電腦的其他人、或這個
 * 網域上的 XSS 都拿得到。但它只能改這幾個分享的設定或刪掉它們，不能讀內容
 * （有密碼的照樣要密碼）、也碰不到帳號，所以換來的方便值得；清單上可一筆一筆移除。
 *
 * 登入後上傳的分享不放這裡 —— 那些歸帳號，在「我的分享」裡看。
 */
export interface RecentShare {
  code: string;
  managementToken: string;
  fileName: string;
  shortUrl: string;
  createdAt: string;
  /** 最後一次知道的設定。有密碼的分享 `GET /shares/{code}` 不回期限跟次數，清單只能靠這份快照顯示；打開管理時讀得到就以 server 的為準。 */
  expiresAt: string | null;
  usesRemaining: number | null;
  hasPassword: boolean;
}

/** 超過就丟最舊的。清單是給「剛剛傳的那個」用的，不是完整的歷史。 */
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
