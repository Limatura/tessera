import { createFileRoute } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";

import { Stagger, StaggerItem } from "@/components/admin/stagger";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { ConfigSummary } from "./-components/config-summary";
import { RecentSharesTable } from "./-components/recent-shares-table";
import { ScannerNotice } from "./-components/scanner-notice";
import { StatsCards } from "./-components/stats-cards";
import { StorageOverview } from "./-components/storage-overview";
import { TrendChart } from "./-components/trend-chart";

export const Route = createFileRoute("/admin/dashboard/")({
  staticData: { title: m.dashboard_title },
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.statsRead
    ),
  component: DashboardComponent,
});

/**
 * Entry needs only `stats:read`. Settings and all shares need their own permissions,
 * and a block the user cannot see is not drawn: it would only be a card that failed to
 * load. When one is missing, the other card in its row fills the width.
 *
 * Two columns only from xl up: the recent shares table is truncated in three fifths of
 * an lg width.
 */
function DashboardComponent() {
  const { data: account } = useSession();
  const canReadSettings = hasPermission(account, PERMISSIONS.settingsRead);
  const canReadShares = hasPermission(account, PERMISSIONS.sharesReadAny);
  const bothLower = canReadSettings && canReadShares;

  return (
    <Stagger className="flex flex-col gap-6">
      {canReadSettings && <ScannerNotice />}
      <StaggerItem>
        <StatsCards />
      </StaggerItem>
      <StaggerItem className="grid grid-cols-1 gap-4 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <TrendChart />
        </div>
        <div className="xl:col-span-2">
          <StorageOverview canReadSettings={canReadSettings} />
        </div>
      </StaggerItem>
      {(canReadShares || canReadSettings) && (
        <StaggerItem className="grid grid-cols-1 gap-4 xl:grid-cols-5">
          {canReadShares && (
            <div className={bothLower ? "xl:col-span-3" : "xl:col-span-5"}>
              <RecentSharesTable />
            </div>
          )}
          {canReadSettings && (
            <div className={bothLower ? "xl:col-span-2" : "xl:col-span-5"}>
              <ConfigSummary />
            </div>
          )}
        </StaggerItem>
      )}
    </Stagger>
  );
}
