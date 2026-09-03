import { createRoute } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import * as React from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { SiteHeader } from "@/components/site-header";

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

const initialData: DemoRow[] = [
  { id: "a", label: "第一項" },
  { id: "b", label: "第二項" },
  { id: "c", label: "第三項" },
  { id: "d", label: "第四項" },
];

const columnHelper = createSortableColumnHelper<DemoRow>();
const columns = columnHelper.columns([
  columnHelper.display({
    cell: ({ row }) => <DragHandle id={row.original.id} />,
    header: () => null,
    id: "drag",
  }),
  columnHelper.accessor("label", { header: "項目" }),
]);

function DndDemoComponent() {
  const [data, setData] = React.useState(initialData);

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
        <SiteHeader title="拖曳排序 Demo" />
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
