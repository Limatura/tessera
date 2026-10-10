import { IconChecks, IconTrash, IconX } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import { cn } from "@tessera/ui/lib/utils";

import type { RowSelection } from "@/hooks/use-row-selection";
import * as m from "@/paraglide/messages";

/**
 * 批次刪除的操作列,「我的分享」跟後台「所有分享」共用。
 *
 * 寬螢幕:固定在表格上方,一直都在 —— 勾第一筆時不會把表格往下推。
 * 窄螢幕:沒勾的時候不出現,勾了才浮在畫面底部,拇指搆得到;
 * 列表底下要自己留 `pb-24`,不然最後一張卡片會被它蓋住。
 */
export function SelectionBar({
  selection,
  deadIds,
  onDelete,
}: {
  selection: RowSelection;
  /** 這一頁已失效的那幾筆,有的話多一顆「選取已失效」。 */
  deadIds: string[];
  onDelete: () => void;
}) {
  const count = selection.selected.length;

  return (
    <section
      aria-label={m.selection_toolbar()}
      className={cn(
        "flex items-center gap-2",
        // 手機:浮動
        "fixed inset-x-4 bottom-[max(1rem,env(safe-area-inset-bottom))] z-40 rounded-xl border bg-popover p-2 pl-4 text-popover-foreground shadow-lg",
        // 桌機:回到文件流裡
        "md:static md:min-h-10 md:border-none md:bg-transparent md:p-0 md:shadow-none",
        count === 0 && "max-md:hidden"
      )}
    >
      <p
        aria-live="polite"
        className="text-sm font-medium tabular-nums md:text-muted-foreground"
      >
        {count > 0 && m.selection_count({ count })}
      </p>
      {count > 0 && (
        <Button
          aria-label={m.selection_clear()}
          onClick={selection.clear}
          size="sm"
          type="button"
          variant="ghost"
        >
          <IconX />
          <span className="max-md:sr-only">{m.selection_clear()}</span>
        </Button>
      )}
      {/* 手機上這條只在有勾選時出現,「選取已失效」放在列表上方那行(`SelectAllRow`)。 */}
      <SelectDeadButton
        className="max-md:hidden"
        deadIds={deadIds}
        selection={selection}
      />
      <Button
        className="ml-auto"
        disabled={count === 0}
        onClick={onDelete}
        size="sm"
        type="button"
        variant="destructive"
      >
        <IconTrash />
        {m.selection_delete()}
      </Button>
    </section>
  );
}

/** 表頭那格的全選框:全選、部分選(—)、沒選三種狀態。 */
export function SelectAllCheckbox({ selection }: { selection: RowSelection }) {
  return (
    <Checkbox
      aria-label={m.selection_select_page()}
      checked={selection.allSelected}
      indeterminate={selection.someSelected}
      onCheckedChange={(checked) => selection.setAll(checked)}
    />
  );
}

/** 手機卡片列表沒有表頭,全選跟「選取已失效」放在列表上面的一行。 */
export function SelectAllRow({
  selection,
  deadIds,
}: {
  selection: RowSelection;
  deadIds: string[];
}) {
  return (
    <div className="flex items-center justify-between gap-2 md:hidden">
      {/* oxlint-disable-next-line jsx-a11y/label-has-associated-control -- Checkbox 是 base-ui 的 button,包在 label 裡就有關聯 */}
      <label className="flex cursor-pointer items-center gap-3 px-1 py-2 text-sm text-muted-foreground">
        <SelectAllCheckbox selection={selection} />
        {m.selection_select_page()}
      </label>
      <SelectDeadButton deadIds={deadIds} selection={selection} />
    </div>
  );
}

/**
 * 一鍵勾起本頁所有已失效的分享。清理時最常做的就是這件事,
 * 逐一挑出灰掉的那幾列太累。本頁沒有失效的就不出現。
 */
function SelectDeadButton({
  selection,
  deadIds,
  className,
}: {
  selection: RowSelection;
  deadIds: string[];
  className?: string;
}) {
  if (deadIds.length === 0) {
    return null;
  }
  return (
    <Button
      className={className}
      onClick={() => selection.selectOnly(deadIds)}
      size="sm"
      type="button"
      variant="ghost"
    >
      <IconChecks />
      {m.selection_select_dead({ count: deadIds.length })}
    </Button>
  );
}
