import type { Variants } from "motion/react";

import { EASE_OUT } from "@/lib/motion";

/** Cards on a page entering one after another (the dashboard). */
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
 * Recharts' draw animation. It starts once the card is halfway in, or the line
 * would finish drawing inside a still transparent card. Callers turn it off
 * with `useReducedMotion()`.
 */
export const CHART_ANIMATION = {
  animationBegin: 250,
  animationDuration: 900,
  animationEasing: "ease-out",
} as const;
