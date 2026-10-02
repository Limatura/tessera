import { createRoute } from "@tanstack/react-router";

import { AdminShell } from "@/components/admin-shell";
import { useSession } from "@/hooks/use-auth";
import { hasPermission, PERMISSIONS } from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

import { Route as rootRoute } from "../__root";
import { ConfigSummary } from "./-components/config-summary";
import { RecentSharesTable } from "./-components/recent-shares-table";
import { StatsCards } from "./-components/stats-cards";
import { StorageOverview } from "./-components/storage-overview";
import { TrendChart } from "./-components/trend-chart";

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

/**
 * 進來只要 `stats:read`。設定跟全站分享各自要自己的權限，沒有的那塊就不畫 ——
 * 畫出來只會是一張載入失敗的卡。少了一塊時，同一列剩下的那張撐滿。
 *
 * 兩欄只在 xl 以上：最近分享的表格擠在 lg 的五分之三寬裡會被截斷。
 */
function DashboardComponent() {
  const { data: account } = useSession();
  const canReadSettings = hasPermission(account, PERMISSIONS.settingsRead);
  const canReadShares = hasPermission(account, PERMISSIONS.sharesReadAny);
  const bothLower = canReadSettings && canReadShares;

  return (
    <AdminShell title={m.dashboard_title()}>
      <StatsCards />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <TrendChart />
        </div>
        <div className="xl:col-span-2">
          <StorageOverview canReadSettings={canReadSettings} />
        </div>
      </div>
      {(canReadShares || canReadSettings) && (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-5">
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
        </div>
      )}
    </AdminShell>
  );
}
