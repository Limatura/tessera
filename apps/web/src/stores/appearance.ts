import { create } from "zustand";
import { persist } from "zustand/middleware";

export const TIME_FORMATS = ["auto", "12h", "24h"] as const;

export type TimeFormat = (typeof TIME_FORMATS)[number];

interface AppearanceState {
  timeFormat: TimeFormat;
  setTimeFormat: (value: TimeFormat) => void;
}

/**
 * Only the time format is left.
 *
 * Theme, accent and mode moved to the server (`hooks/use-preferences.ts`): they
 * sync across devices, and the owner's site default must be able to override
 * them, neither of which works from one browser's storage.
 *
 * Time format stays because it has no server field yet. The `user_preferences`
 * table was named to hold language, time zone and time format later; this moves
 * there when that lands.
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
