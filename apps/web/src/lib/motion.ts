import type { Transition, Variants } from "motion/react";

/** The ease-out-expo curve shared by the site and the admin panel. */
export const EASE_OUT: Transition["ease"] = [0.16, 1, 0.3, 1];

/**
 * Page content entering on navigation: a fade in only. No exit animation, so a
 * new page does not wait for the old one to finish.
 *
 * No movement: a transform counts toward scrollable height, so a page about one
 * screen tall flashes a scrollbar on every navigation, and a transform also
 * makes this layer the containing block for `fixed` backgrounds inside it.
 */
export const pageVariants: Variants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: { duration: 0.45, ease: EASE_OUT },
  },
};

/**
 * The entrance shared by the sign-in, registration and password reset forms:
 * the outer layer staggers, each field fades in and rises in turn. Kept here so
 * the three stay identical.
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
