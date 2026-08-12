import { motion } from "motion/react";

export function GlassBackground() {
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none fixed inset-0 -z-10 overflow-hidden bg-background"
    >
      <motion.div
        animate={{
          opacity: 1,
          scale: [1, 1.08, 1],
          x: [0, 30, 0],
          y: [0, 20, 0],
        }}
        className="-translate-x-1/2 absolute top-[-15%] left-1/2 h-[46rem] w-[46rem] rounded-full bg-primary/35 blur-[110px]"
        initial={{ opacity: 0, scale: 0.9 }}
        transition={{
          opacity: { duration: 1.8, ease: "easeOut" },
          scale: {
            duration: 10,
            ease: "easeInOut",
            repeat: Number.POSITIVE_INFINITY,
          },
          x: {
            duration: 10,
            ease: "easeInOut",
            repeat: Number.POSITIVE_INFINITY,
          },
          y: {
            duration: 10,
            ease: "easeInOut",
            repeat: Number.POSITIVE_INFINITY,
          },
        }}
      />
      <motion.div
        animate={{
          opacity: 1,
          scale: [1, 1.1, 1],
          x: [0, -25, 0],
          y: [0, -15, 0],
        }}
        className="absolute right-[-12%] bottom-[-18%] h-[36rem] w-[36rem] rounded-full bg-chart-2/30 blur-[110px]"
        initial={{ opacity: 0, scale: 0.9 }}
        transition={{
          opacity: { delay: 0.2, duration: 1.8, ease: "easeOut" },
          scale: {
            duration: 11,
            ease: "easeInOut",
            repeat: Number.POSITIVE_INFINITY,
          },
          x: {
            duration: 11,
            ease: "easeInOut",
            repeat: Number.POSITIVE_INFINITY,
          },
          y: {
            duration: 11,
            ease: "easeInOut",
            repeat: Number.POSITIVE_INFINITY,
          },
        }}
      />
      <motion.div
        animate={{ opacity: 1 }}
        className="-translate-x-1/2 -translate-y-1/2 absolute top-1/2 left-1/2 h-[24rem] w-[24rem] rounded-full bg-primary/15 blur-[90px]"
        initial={{ opacity: 0 }}
        transition={{ delay: 0.4, duration: 1.8, ease: "easeOut" }}
      />

      <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,transparent_0%,var(--color-background)_92%)]" />
    </div>
  );
}
