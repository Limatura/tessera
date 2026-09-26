import { createRoute } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import * as React from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { SiteHeader } from "@/components/site-header";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import {
  createSortableColumnHelper,
  DragHandle,
  SortableTable,
} from "./-components/sortable-table";

export const Route = createRoute({
  component: DndDemoComponent,
  getParentRoute: () => rootRoute,
  path: "/dnd-demo",
});

interface DemoRow {
  id: string;
  label: string;
}

function buildInitialData(): DemoRow[] {
  return [
    { id: "a", label: m.dnd_demo_item_one() },
    { id: "b", label: m.dnd_demo_item_two() },
    { id: "c", label: m.dnd_demo_item_three() },
    { id: "d", label: m.dnd_demo_item_four() },
  ];
}

const columnHelper = createSortableColumnHelper<DemoRow>();

function buildColumns() {
  return columnHelper.columns([
    columnHelper.display({
      cell: ({ row }) => <DragHandle id={row.original.id} />,
      header: () => null,
      id: "drag",
    }),
    columnHelper.accessor("label", { header: m.dnd_demo_column_item() }),
  ]);
}

function DndDemoComponent() {
  const [data, setData] = React.useState(buildInitialData);
  const columns = React.useMemo(() => buildColumns(), []);

  return (
    <SidebarProvider
      style={
        {
          "--header-height": "calc(var(--spacing) * 12)",
          "--sidebar-width": "calc(var(--spacing) * 72)",
        } as React.CSSProperties
      }
    >
      <AppSidebar variant="inset" />
      <SidebarInset>
        <SiteHeader title={m.dnd_demo_title()} />
        <div className="p-4 lg:p-6">
          <SortableTable
            data={data}
            columns={columns}
            getRowId={(row) => row.id}
            onReorder={(from, to) => {
              setData((current) => {
                const next = [...current];
                const [moved] = next.splice(from, 1);
                next.splice(to, 0, moved);
                return next;
              });
            }}
          />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
