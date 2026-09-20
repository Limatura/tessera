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
      label: "分享總數",
      sub: `可取件 ${number(stats.liveShares)} 筆`,
      value: number(stats.shares),
    },
    {
      icon: IconDatabase,
      label: "已用儲存空間",
      sub: stats.quotaBytes
        ? `配額 ${formatBytes(stats.quotaBytes)}`
        : "無配額限制",
      value: formatBytes(stats.storedBytes),
    },
    {
      icon: IconCloudUpload,
      label: "帳號數",
      sub: "含管理員",
      value: number(stats.accounts),
    },
    {
      icon: IconTicket,
      label: "可用邀請碼",
      sub: "尚未用盡或過期",
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
