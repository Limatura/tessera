import { motion } from "motion/react";
import type * as React from "react";

import { pageVariants } from "@/lib/admin/motion";

/**
 * 內容區換頁動畫。`transitionKey` 一變就重掛載並重播進場。
 *
 * 呼叫端決定 key 的粒度：外框用「後台第幾個區塊」，所以設定頁內切分頁不會連整個
 * 設定外框一起重播，只有各自的內層會動。
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
