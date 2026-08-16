import { createRoute } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import type * as React from "react";
import type { z } from "zod";

import { AppSidebar } from "@/components/app-sidebar";
import { ChartAreaInteractive } from "@/components/chart-area-interactive";
import { DataTable } from "@/components/data-table";
import type { schema } from "@/components/data-table";
import { SectionCards } from "@/components/section-cards";
import { SiteHeader } from "@/components/site-header";
import dashboardData from "@/data/dashboard.json";

import { Route as rootRoute } from "./__root";

export const Route = createRoute({
  component: DashboardComponent,
  getParentRoute: () => rootRoute,
  path: "/dashboard",
});

function DashboardComponent() {
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
        <SiteHeader />
        <div className="flex flex-1 flex-col">
          <div className="@container/main flex flex-1 flex-col gap-2">
            <div className="flex flex-col gap-4 py-4 md:gap-6 md:py-6">
              <SectionCards />
              <div className="px-4 lg:px-6">
                <ChartAreaInteractive />
              </div>
              <DataTable data={dashboardData as z.infer<typeof schema>[]} />
            </div>
          </div>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
