import { createRoute } from "@tanstack/react-router";

import { AdminShell } from "@/components/admin-shell";
import { PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { ConfigSummary } from "./-components/config-summary";
import { RecentSharesTable } from "./-components/recent-shares-table";
import { StatsCards } from "./-components/stats-cards";
import { StorageTrendChart } from "./-components/storage-trend-chart";

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.statsRead
    ),
  component: DashboardComponent,
  getParentRoute: () => rootRoute,
  path: "/dashboard",
});

function DashboardComponent() {
  return (
    <AdminShell title={m.dashboard_title()}>
      <StatsCards />
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <StorageTrendChart />
        </div>
        <ConfigSummary />
      </div>
      <RecentSharesTable />
    </AdminShell>
  );
}
