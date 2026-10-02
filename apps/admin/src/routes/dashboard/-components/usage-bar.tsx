import { cn } from "@tessera/ui/lib/utils";

/** 過了這條線要開始安排，過了下一條就是快滿了。 */
export const WARN_PERCENT = 80;
const CRITICAL_PERCENT = 95;

/**
 * 用量佔上限的百分比，四捨五入到整數。
 *
 * 不封頂：上限在併發下是近似值，真的超過時要讓人看到 103%，而不是停在 100%。
 */
export function usagePercent(used: number, total: number): number {
  if (total <= 0) {
    return 0;
  }
  return Math.round((used / total) * 100);
}

/**
 * 一條用量長條。沒有上限的東西不要畫它 —— 對「無限」的比例沒有意義。
 *
 * 純裝飾：旁邊一定已經寫著「已用 / 上限」，螢幕閱讀器不必再念一次。
 */
export function UsageBar({ used, total }: { used: number; total: number }) {
  const percent = usagePercent(used, total);

  return (
    <div
      aria-hidden
      className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
    >
      <div
        className={cn(
          "h-full rounded-full transition-[width]",
          percent >= CRITICAL_PERCENT && "bg-destructive",
          percent >= WARN_PERCENT &&
            percent < CRITICAL_PERCENT &&
            "bg-amber-500",
          percent < WARN_PERCENT && "bg-primary"
        )}
        style={{ width: `${Math.min(percent, 100)}%` }}
      />
    </div>
  );
}
