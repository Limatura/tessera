import { IconChecks, IconTrash, IconX } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import { cn } from "@tessera/ui/lib/utils";

import type { RowSelection } from "@/hooks/use-row-selection";
import * as m from "@/paraglide/messages";

/**
 * Bulk delete bar, shared by "My shares" and the admin "All shares".
 *
 * Wide screens: fixed above the table and always present, so ticking the first
 * row does not push the table down.
 * Narrow screens: hidden until something is ticked, then floats at the bottom
 * within thumb reach. The list must leave `pb-24` below itself, or the bar
 * covers the last card.
 */
export function SelectionBar({
  selection,
  deadIds,
  onDelete,
}: {
  selection: RowSelection;
  /** This page's dead rows; if there are any, a "select dead" button is added. */
  deadIds: string[];
  onDelete: () => void;
}) {
  const count = selection.selected.length;

  return (
    <section
      aria-label={m.selection_toolbar()}
      className={cn(
        "flex items-center gap-2",
        // Mobile: floating
        "fixed inset-x-4 bottom-[max(1rem,env(safe-area-inset-bottom))] z-40 rounded-xl border bg-popover p-2 pl-4 text-popover-foreground shadow-lg",
        // Desktop: back in the document flow
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
      {/* On mobile this bar only appears with a selection, so "select dead"
         lives in the row above the list (`SelectAllRow`). */}
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

/**
 * The select-all box in the table header, with three states: all, some
 * (indeterminate) and none.
 */
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

/**
 * Mobile card lists have no header, so select-all and "select dead" sit in a
 * row above the list.
 */
export function SelectAllRow({
  selection,
  deadIds,
}: {
  selection: RowSelection;
  deadIds: string[];
}) {
  return (
    <div className="flex items-center justify-between gap-2 md:hidden">
      {/* oxlint-disable-next-line jsx-a11y/label-has-associated-control --
         Checkbox is a base-ui button, so wrapping it in the label associates
         them */}
      <label className="flex cursor-pointer items-center gap-3 px-1 py-2 text-sm text-muted-foreground">
        <SelectAllCheckbox selection={selection} />
        {m.selection_select_page()}
      </label>
      <SelectDeadButton deadIds={deadIds} selection={selection} />
    </div>
  );
}

/**
 * Ticks every dead share on this page at once. It is the most common cleanup
 * action, and picking out the greyed rows one by one is tedious. Hidden when
 * the page has no dead shares.
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
