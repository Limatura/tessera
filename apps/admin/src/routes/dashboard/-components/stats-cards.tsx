import {
  IconDatabase,
  IconFiles,
  IconSend,
  IconUsers,
} from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Skeleton } from "@tessera/ui/components/skeleton";

import { useStats } from "@/hooks/use-admin";
import { formatBytes, formatNumber } from "@/lib/format";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { usagePercent } from "./usage-bar";

type Stats = components["schemas"]["StatsResponse"];

/** 載入中先佔四格，版面不會在資料回來時跳一下。 */
const STAT_PLACEHOLDERS = ["shares", "stored", "served", "accounts"] as const;

/** 數字要跟著介面語言走，所以在元件裡建（`getLocale()` 是執行期的值）。 */
function buildItems(stats: Stats) {
  const locale = getLocale();
  const number = (value: number) => formatNumber(value, locale);
  const quota = stats.quota_bytes;

  return [
    {
      icon: IconFiles,
      label: m.dashboard_stats_shares(),
      sub: m.dashboard_stats_shares_live({
        count: number(stats.live_shares),
      }),
      value: number(stats.shares),
    },
    {
      icon: IconDatabase,
      label: m.dashboard_stats_stored(),
      // `null` 是沒設，不是無限 —— 真的沒有上限時就照實說，不要畫一個對 0 的比例。
      sub:
        quota === null || quota === undefined
          ? m.dashboard_stats_stored_no_quota()
          : m.dashboard_stats_stored_quota({
              amount: formatBytes(quota),
              percent: number(usagePercent(stats.stored_bytes, quota)),
            }),
      value: formatBytes(stats.stored_bytes),
    },
    {
      icon: IconSend,
      label: m.dashboard_stats_served(),
      // 直連的是「被要求」的量，不是真的送達的量，所以不加進主數字，另外列。
      sub:
        stats.bytes_redirected > 0
          ? m.dashboard_stats_served_redirected({
              amount: formatBytes(stats.bytes_redirected),
            })
          : m.dashboard_stats_served_direct(),
      value: formatBytes(stats.bytes_served),
    },
    {
      icon: IconUsers,
      label: m.dashboard_stats_accounts(),
      sub: m.dashboard_stats_accounts_detail({
        count: number(stats.administrators),
      }),
      value: number(stats.accounts),
    },
  ];
}

export function StatsCards() {
  const stats = useStats();

  if (stats.isError) {
    return (
      <p className="text-destructive text-sm">
        {m.dashboard_stats_load_error()}
      </p>
    );
  }

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
      {stats.data
        ? buildItems(stats.data).map((item) => (
            <Card key={item.label} className="ring-foreground/10">
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardDescription>{item.label}</CardDescription>
                  <item.icon className="size-4 text-muted-foreground" />
                </div>
                <CardTitle className="font-semibold text-2xl tabular-nums">
                  {item.value}
                </CardTitle>
              </CardHeader>
              <p className="px-(--card-spacing) text-muted-foreground text-xs">
                {item.sub}
              </p>
            </Card>
          ))
        : STAT_PLACEHOLDERS.map((key) => (
            <Skeleton key={key} className="h-28 rounded-xl" />
          ))}
    </div>
  );
}
