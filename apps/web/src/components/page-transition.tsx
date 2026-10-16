import { motion } from "motion/react";
import type * as React from "react";

import { pageVariants } from "@/lib/motion";

/**
 * Page transition for the content area. Changing `transitionKey` remounts and
 * replays the entrance.
 *
 * The caller picks the key's granularity. The admin frame keys on the section,
 * so switching tabs inside settings replays only each tab's inner content, not
 * the settings frame.
 */
export function PageTransition({
  children,
  className,
  transitionKey,
}: {
  children: React.ReactNode;
  className?: string;
  transitionKey: string;
}) {
  return (
    <motion.div
      animate="visible"
      className={className}
      initial="hidden"
      key={transitionKey}
      variants={pageVariants}
    >
      {children}
    </motion.div>
  );
}
