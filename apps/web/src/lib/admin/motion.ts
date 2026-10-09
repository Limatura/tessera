import type { Variants } from "motion/react";

import { EASE_OUT } from "@/lib/motion";

/** 一頁裡的多張卡片依序進場（儀表板）。 */
export const staggerVariants: Variants = {
  hidden: {},
  visible: { transition: { delayChildren: 0.05, staggerChildren: 0.1 } },
};

export const staggerItemVariants: Variants = {
  hidden: { opacity: 0, y: 16 },
  visible: {
    opacity: 1,
    transition: { duration: 0.7, ease: EASE_OUT },
    y: 0,
  },
};

/**
 * Recharts 的描繪動畫。等卡片浮上來一半再開始畫，不然線條在透明的卡片裡就畫完了。
 * 減少動態效果時由呼叫端用 `useReducedMotion()` 關掉。
 */
export const CHART_ANIMATION = {
  animationBegin: 250,
  animationDuration: 900,
  animationEasing: "ease-out",
} as const;
