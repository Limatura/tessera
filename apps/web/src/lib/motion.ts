import type { Variants } from "motion/react";

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
    transition: { duration: 0.5, ease: [0.16, 1, 0.3, 1] },
    y: 0,
  },
};
