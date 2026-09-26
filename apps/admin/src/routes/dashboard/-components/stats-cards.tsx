import {
  IconCloudUpload,
  IconDatabase,
  IconFiles,
  IconTicket,
} from "@tabler/icons-react";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";

import mockData from "@/data/dashboard-mock.json";
import { formatBytes, formatNumber } from "@/lib/format";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

/**
 * Mirrors the server's `StatsResponse` (apps/server/src/api/admin.rs).
 * Backed by static mock data until GET /admin/stats is wired up.
 */
const { stats } = mockData;

/**
 * 數字要跟著介面語言走，所以在元件裡建（`getLocale()` 是執行期的值）。
 */
function buildItems() {
  const locale = getLocale();
  const number = (value: number) => formatNumber(value, locale);

  return [
    {
      icon: IconFiles,
      label: m.dashboard_stats_total_shares(),
      sub: m.dashboard_stats_live_shares({ count: number(stats.liveShares) }),
      value: number(stats.shares),
    },
    {
      icon: IconDatabase,
      label: m.dashboard_stats_used_storage(),
      sub: stats.quotaBytes
        ? m.dashboard_stats_quota({ amount: formatBytes(stats.quotaBytes) })
        : m.dashboard_stats_no_quota(),
      value: formatBytes(stats.storedBytes),
    },
    {
      icon: IconCloudUpload,
      label: m.dashboard_stats_accounts(),
      sub: m.dashboard_stats_accounts_sub(),
      value: number(stats.accounts),
    },
    {
      icon: IconTicket,
      label: m.dashboard_stats_available_invites(),
      sub: m.dashboard_stats_available_invites_sub(),
      value: number(stats.liveInvites),
    },
  ];
}

export function StatsCards() {
  const items = buildItems();

  return (
    <div className="grid grid-cols-1 gap-4 px-4 sm:grid-cols-2 lg:px-6 xl:grid-cols-4">
      {items.map((item) => (
        <Card key={item.label} className="ring-foreground/10">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardDescription>{item.label}</CardDescription>
              <item.icon className="size-4 text-muted-foreground" />
            </div>
            <CardTitle className="text-2xl font-semibold tabular-nums">
              {item.value}
            </CardTitle>
          </CardHeader>
          <p className="px-(--card-spacing) text-xs text-muted-foreground">
            {item.sub}
          </p>
        </Card>
      ))}
    </div>
  );
}
