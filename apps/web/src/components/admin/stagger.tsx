import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import type * as React from "react";

import { staggerItemVariants, staggerVariants } from "@/lib/admin/motion";

/**
 * A group of blocks that enter in sequence (the dashboard's card rows); children
 * use `StaggerItem`.
 *
 * Children slide up as they enter. `overflow-clip` keeps them inside the
 * container, or the last row would add a scrollbar. The clip margin leaves room
 * for the ring around each card.
 */
export function Stagger({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <motion.div
      animate="visible"
      className={cn("overflow-clip [overflow-clip-margin:4px]", className)}
      initial="hidden"
      variants={staggerVariants}
    >
      {children}
    </motion.div>
  );
}

export function StaggerItem({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <motion.div className={className} variants={staggerItemVariants}>
      {children}
    </motion.div>
  );
}
