import { create } from "zustand";
import { persist } from "zustand/middleware";

export const APP_THEMES = ["default", "graphite", "sepia"] as const;
export const ACCENT_COLORS = [
  "violet",
  "rose",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
] as const;
export const TIME_FORMATS = ["auto", "12h", "24h"] as const;

export type AppTheme = (typeof APP_THEMES)[number];
export type AccentColor = (typeof ACCENT_COLORS)[number];
export type TimeFormat = (typeof TIME_FORMATS)[number];

interface AppearanceState {
  appTheme: AppTheme;
  accentColor: AccentColor;
  timeFormat: TimeFormat;
  setAppTheme: (value: AppTheme) => void;
  setAccentColor: (value: AccentColor) => void;
  setTimeFormat: (value: TimeFormat) => void;
}

/**
 * 偏好設定頁的外觀選項。目前只存在這台瀏覽器 —— 後端還沒有 user preferences 端點,
 * 接上之後這裡改成只放「還沒送出」的暫存值,真值由 TanStack Query 持有。
 *
 * 明暗主題不在這裡:那個歸 next-themes 管(`useTheme`)。
 */
export const useAppearanceStore = create<AppearanceState>()(
  persist(
    (set) => ({
      accentColor: "violet",
      appTheme: "default",
      setAccentColor: (value) => set({ accentColor: value }),
      setAppTheme: (value) => set({ appTheme: value }),
      setTimeFormat: (value) => set({ timeFormat: value }),
      timeFormat: "auto",
    }),
    { name: "tessera-appearance" }
  )
);
