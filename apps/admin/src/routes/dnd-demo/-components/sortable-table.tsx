import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  MouseSensor,
  TouchSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import type { DragEndEvent, UniqueIdentifier } from "@dnd-kit/core";
import { restrictToVerticalAxis } from "@dnd-kit/modifiers";
import {
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { IconGripVertical } from "@tabler/icons-react";
import {
  columnVisibilityFeature,
  createColumnHelper,
  FlexRender,
  rowSelectionFeature,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import type { ColumnDef, Row, RowData } from "@tanstack/react-table";
import { Button } from "@tessera/ui/components/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import * as React from "react";

/**
 * The feature set every sortable table registers. Fixed rather than
 * generic over the caller: v9's `Row<TFeatures, TData>` produces a union
 * TypeScript can't distribute method lookups through once `TFeatures` is a
 * type parameter, so this module owns one concrete `tableFeatures()` call
 * and callers build columns with the `columnHelper` it exports.
 */
export const sortableTableFeatures = tableFeatures({
  columnVisibilityFeature,
  rowSelectionFeature,
});

export function createSortableColumnHelper<TData extends RowData>() {
  return createColumnHelper<typeof sortableTableFeatures, TData>();
}

/** Drag handle cell — put this as the `cell` of a `columnHelper.display({ id: "drag" })` column. */
export function DragHandle({ id }: { id: UniqueIdentifier }) {
  const { attributes, listeners } = useSortable({ id });
  return (
    <Button
      {...attributes}
      {...listeners}
      variant="ghost"
      size="icon"
      className="size-7 text-muted-foreground hover:bg-transparent"
    >
      <IconGripVertical className="size-3 text-muted-foreground" />
      <span className="sr-only">拖曳排序</span>
    </Button>
  );
}

function DraggableRow<TData extends RowData>({
  row,
}: {
  row: Row<typeof sortableTableFeatures, TData>;
}) {
  const { transform, transition, setNodeRef, isDragging } = useSortable({
    id: row.id,
  });
  return (
    <TableRow
      data-state={row.getIsSelected() && "selected"}
      data-dragging={isDragging}
      ref={setNodeRef}
      className="relative z-0 data-[dragging=true]:z-10 data-[dragging=true]:opacity-80"
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
      }}
    >
      {row.getVisibleCells().map((cell) => (
        <TableCell key={cell.id}>
          <FlexRender cell={cell} />
        </TableCell>
      ))}
    </TableRow>
  );
}

/**
 * A table with dnd-kit vertical drag-to-reorder wired up. Build `columns`
 * with `createSortableColumnHelper<TData>()` (include a `"drag"` display
 * column rendering `<DragHandle id={...} />`), keep `data` in state, and
 * this component drives the reorder on drop.
 */
export function SortableTable<TData extends RowData>({
  data,
  columns,
  getRowId,
  onReorder,
  emptyMessage = "沒有資料",
}: {
  data: TData[];
  columns: ColumnDef<typeof sortableTableFeatures, TData>[];
  getRowId: (row: TData) => UniqueIdentifier;
  onReorder: (fromIndex: number, toIndex: number) => void;
  emptyMessage?: string;
}) {
  const sortableId = React.useId();
  const sensors = useSensors(
    useSensor(MouseSensor, {}),
    useSensor(TouchSensor, {}),
    useSensor(KeyboardSensor, {})
  );
  const [rowSelection, setRowSelection] = React.useState({});
  const [columnVisibility, setColumnVisibility] = React.useState({});

  const table = useTable({
    columns,
    data,
    enableRowSelection: true,
    features: sortableTableFeatures,
    getRowId: (row) => getRowId(row).toString(),
    onColumnVisibilityChange: setColumnVisibility,
    onRowSelectionChange: setRowSelection,
    state: { columnVisibility, rowSelection },
  });

  const rowIds = React.useMemo(() => data.map(getRowId), [data, getRowId]);

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (active && over && active.id !== over.id) {
      const oldIndex = rowIds.indexOf(active.id);
      const newIndex = rowIds.indexOf(over.id);
      onReorder(oldIndex, newIndex);
    }
  }

  const { rows } = table.getRowModel();

  return (
    <div className="overflow-hidden rounded-lg border">
      <DndContext
        collisionDetection={closestCenter}
        modifiers={[restrictToVerticalAxis]}
        onDragEnd={handleDragEnd}
        sensors={sensors}
        id={sortableId}
      >
        <Table>
          <TableHeader className="sticky top-0 z-10 bg-muted">
            {table.getHeaderGroups().map((headerGroup) => (
              <TableRow key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <TableHead key={header.id} colSpan={header.colSpan}>
                    {header.isPlaceholder ? null : (
                      <FlexRender header={header} />
                    )}
                  </TableHead>
                ))}
              </TableRow>
            ))}
          </TableHeader>
          <TableBody className="**:data-[slot=table-cell]:first:w-8">
            {rows.length ? (
              <SortableContext
                items={rowIds}
                strategy={verticalListSortingStrategy}
              >
                {rows.map((row) => (
                  <DraggableRow key={row.id} row={row} />
                ))}
              </SortableContext>
            ) : (
              <TableRow>
                <TableCell
                  colSpan={columns.length}
                  className="h-24 text-center"
                >
                  {emptyMessage}
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </DndContext>
    </div>
  );
}
