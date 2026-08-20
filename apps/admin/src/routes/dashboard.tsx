import { createRoute } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import type * as React from "react";

import { AppSidebar } from "@/components/app-sidebar";
import { ConfigSummary } from "@/components/config-summary";
import { RecentSharesTable } from "@/components/recent-shares-table";
import { SiteHeader } from "@/components/site-header";
import { StatsCards } from "@/components/stats-cards";
import { StorageTrendChart } from "@/components/storage-trend-chart";

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
        <SiteHeader title="儀表板" />
        <div className="flex flex-1 flex-col">
          <div className="@container/main flex flex-1 flex-col gap-6 py-4 md:py-6">
            <StatsCards />
            <div className="grid grid-cols-1 gap-4 px-4 lg:grid-cols-3 lg:px-6">
              <div className="lg:col-span-2">
                <StorageTrendChart />
              </div>
              <ConfigSummary />
            </div>
            <div className="px-4 lg:px-6">
              <RecentSharesTable />
            </div>
          </div>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
