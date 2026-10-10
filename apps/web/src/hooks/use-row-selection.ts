import { useState } from "react";

const NONE: ReadonlySet<string> = new Set();

/**
 * 表格／卡片列表的勾選狀態,只管「這一頁」。
 *
 * 選取跟 `scope`(通常是頁碼)綁在一起:換頁就等於清空,回到上一頁也不會冒出
 * 之前勾的。不在 `rowIds` 裡的 id 一律不算 —— 刪掉或被別人清掉的那幾列重新整理後
 * 就消失,不會留著「已選 3 筆」卻只看得到 1 筆。
 */
export function useRowSelection(rowIds: readonly string[], scope: unknown) {
  const [state, setState] = useState<{
    scope: unknown;
    ids: ReadonlySet<string>;
  }>({ ids: NONE, scope });

  const current = state.scope === scope ? state.ids : NONE;
  const selected = rowIds.filter((id) => current.has(id));
  const allSelected = rowIds.length > 0 && selected.length === rowIds.length;

  const replace = (ids: Iterable<string>) =>
    setState({ ids: new Set(ids), scope });

  return {
    allSelected,
    clear: () => replace([]),
    /** 一次取消好幾個,例如刪掉的那幾筆。連續呼叫 `setOne` 會互相蓋掉,因為都從同一次 render 的狀態算。 */
    deselect: (ids: readonly string[]) =>
      replace(selected.filter((id) => !ids.includes(id))),
    isSelected: (id: string) => current.has(id),
    /** 部分勾選:表頭的全選框顯示成「—」。 */
    someSelected: selected.length > 0 && !allSelected,
    selected,
    /** 只選這些(例如「選取已失效」),原本勾的取消。 */
    selectOnly: (ids: readonly string[]) =>
      replace(rowIds.filter((id) => ids.includes(id))),
    setAll: (checked: boolean) => replace(checked ? rowIds : []),
    setOne: (id: string, checked: boolean) => {
      const next = new Set(selected);
      if (checked) {
        next.add(id);
      } else {
        next.delete(id);
      }
      replace(next);
    },
  };
}

export type RowSelection = ReturnType<typeof useRowSelection>;
