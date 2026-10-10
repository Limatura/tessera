import { IconInfoCircle } from "@tabler/icons-react";

/**
 * 頁面上有已失效的分享時,說明它們為什麼還在 —— 不講的話,看到過期的還掛在列表上
 * 會以為清理壞了。
 *
 * 兩頁講的深度不同,所以文字由外面給:「我的分享」只要知道會自己消失、不想等可以刪;
 * 後台要知道檔案已經不佔空間、紀錄為什麼還留著(`share.retain_after_death_hours`),
 * 每一列的移除日期由 `PurgeDate` 標在狀態旁邊。
 */
export function DeadSharesNote({ children }: { children: React.ReactNode }) {
  return (
    <p className="flex items-start gap-2 text-muted-foreground text-xs">
      <IconInfoCircle className="mt-px shrink-0" size={14} />
      {children}
    </p>
  );
}
