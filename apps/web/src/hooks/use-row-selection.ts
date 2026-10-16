import { useState } from "react";

const NONE: ReadonlySet<string> = new Set();

/**
 * Selection state for a table or card list, for the current page only.
 *
 * Selection is tied to `scope` (usually the page number): changing page clears
 * it, and going back does not bring old ticks back. Ids not in `rowIds` never
 * count, so rows deleted here or by someone else vanish on refresh instead of
 * leaving "3 selected" with only 1 visible.
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
    /**
     * Deselects several at once, such as the deleted rows. Repeated `setOne`
     * calls overwrite each other, since each computes from the same render's
     * state.
     */
    deselect: (ids: readonly string[]) =>
      replace(selected.filter((id) => !ids.includes(id))),
    isSelected: (id: string) => current.has(id),
    /** Partly selected: the header's select-all box shows as indeterminate. */
    someSelected: selected.length > 0 && !allSelected,
    selected,
    /** Selects only these (for "select dead"), deselecting the rest. */
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
