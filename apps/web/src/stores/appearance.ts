import { create } from "zustand";
import { persist } from "zustand/middleware";

export const TIME_FORMATS = ["auto", "12h", "24h"] as const;

export type TimeFormat = (typeof TIME_FORMATS)[number];

interface AppearanceState {
  timeFormat: TimeFormat;
  setTimeFormat: (value: TimeFormat) => void;
}

/**
 * 只剩時間格式。
 *
 * 主題、強調色、明暗都搬去伺服器了(`hooks/use-preferences.ts`)—— 那三個要跨
 * 裝置同步,而且擁有者定的站台預設要蓋得過去,存在單一瀏覽器裡辦不到。
 *
 * 時間格式留在這裡是因為它還沒有對應的後端欄位;`user_preferences` 表就是為了
 * 之後放語言、時區、時間格式而取這個名字的,接上時這裡也會跟著搬過去。
 */
export const useAppearanceStore = create<AppearanceState>()(
  persist(
    (set) => ({
      setTimeFormat: (value) => set({ timeFormat: value }),
      timeFormat: "auto",
    }),
    { name: "tessera-appearance" }
  )
);
