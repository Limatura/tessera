import type { Transition, Variants } from "motion/react";

/** 前後台共用的 ease-out-expo 曲線。 */
export const EASE_OUT: Transition["ease"] = [0.16, 1, 0.3, 1];

/**
 * 換頁時內容區的進場：只淡入。只做進場不做退場，新頁面不用等舊頁面播完。
 *
 * 不做位移 —— transform 會算進可捲動高度，頁面剛好接近一屏高時每次換頁都會閃出
 * scrollbar；而且 transform 會讓頁面裡 `fixed` 的背景改以這層為定位基準。
 */
export const pageVariants: Variants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: { duration: 0.45, ease: EASE_OUT },
  },
};

/**
 * 登入/註冊/重設密碼三張表單共用的進場動畫:外層做 stagger,
 * 內層每個欄位依序淡入上移。三處要一致,所以放這裡而不是各自宣告。
 */
export const containerVariants: Variants = {
  hidden: {},
  visible: {
    transition: { delayChildren: 0.1, staggerChildren: 0.06 },
  },
};

export const itemVariants: Variants = {
  hidden: { opacity: 0, y: 10 },
  visible: {
    opacity: 1,
    transition: { duration: 0.5, ease: EASE_OUT },
    y: 0,
  },
};
