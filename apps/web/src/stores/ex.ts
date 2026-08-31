import { create } from "zustand";

interface AdminShortcutState {
  overridePath: string | null;
  setOverridePath: (path: string | null) => void;
}

export const useAdminShortcutStore = create<AdminShortcutState>((set) => ({
  overridePath: null,
  setOverridePath: (path) => set({ overridePath: path }),
}));
