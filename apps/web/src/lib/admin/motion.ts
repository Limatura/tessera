import type { Transition, Variants } from "motion/react";

/** 跟前台表單同一條 ease-out-expo 曲線，後台只是縮短時間：操作型介面不該讓人等動畫。 */
export const EASE_OUT: Transition["ease"] = [0.16, 1, 0.3, 1];

/**
 * 換頁時內容區的進場：只淡入。只做進場不做退場，新頁面不用等舊頁面播完。
 *
 * 不做位移 —— transform 會算進可捲動高度，頁面剛好接近一屏高時每次換頁都會閃出
 * scrollbar。
 */
export const pageVariants: Variants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: { duration: 0.45, ease: EASE_OUT },
  },
};

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
