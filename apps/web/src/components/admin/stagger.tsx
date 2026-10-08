import { cn } from "@tessera/ui/lib/utils";
import { motion } from "motion/react";
import type * as React from "react";

import { staggerItemVariants, staggerVariants } from "@/lib/admin/motion";

/**
 * 一組依序進場的區塊（儀表板的卡片列）；子層用 `StaggerItem`。
 *
 * 子層進場會往上位移，`overflow-clip` 把它關在容器裡，不然最後一列會撐出
 * scrollbar。留一點 clip margin，卡片外圈的 ring 才不會被切掉。
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
