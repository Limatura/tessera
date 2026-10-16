import NumberFlow from "@number-flow/react";
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
import { formatBytes, formatNumber, splitBytes } from "@tessera/ui/lib/format";
import { useEffect, useState } from "react";

import { usagePercent } from "@/components/admin/usage-bar";
import { useStats } from "@/hooks/admin/use-stats";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

type Stats = components["schemas"]["StatsResponse"];

/** Four placeholders while loading, so the layout does not jump when data arrives. */
const STAT_PLACEHOLDERS = ["shares", "stored", "served", "accounts"] as const;

/** The big number on a card: NumberFlow animates the value, the unit is a fixed suffix. */
interface StatValue {
  value: number;
  fractionDigits: number;
  suffix?: string;
}

function countValue(value: number): StatValue {
  return { fractionDigits: 0, value };
}

function bytesValue(bytes: number): StatValue {
  const { value, unit, fractionDigits } = splitBytes(bytes);
  return { fractionDigits, suffix: ` ${unit}`, value };
}

/**
 * Numbers follow the UI language, so the formatter is built in the component:
 * `getLocale()` is a runtime value.
 */
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
      value: countValue(stats.shares),
    },
    {
      icon: IconDatabase,
      label: m.dashboard_stats_stored(),
      // `null` means no quota is set. Say so plainly instead of drawing a ratio against 0.
      sub:
        quota === null || quota === undefined
          ? m.dashboard_stats_stored_no_quota()
          : m.dashboard_stats_stored_quota({
              amount: formatBytes(quota),
              percent: number(usagePercent(stats.stored_bytes, quota)),
            }),
      value: bytesValue(stats.stored_bytes),
    },
    {
      icon: IconSend,
      label: m.dashboard_stats_served(),
      // Redirected bytes are what was requested, not delivered, so they are listed
      // separately instead of added to the main figure.
      sub:
        stats.bytes_redirected > 0
          ? m.dashboard_stats_served_redirected({
              amount: formatBytes(stats.bytes_redirected),
            })
          : m.dashboard_stats_served_direct(),
      value: bytesValue(stats.bytes_served),
    },
    {
      icon: IconUsers,
      label: m.dashboard_stats_accounts(),
      sub: m.dashboard_stats_accounts_detail({
        count: number(stats.administrators),
      }),
      value: countValue(stats.accounts),
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
                  <AnimatedStat {...item.value} />
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

/**
 * Counts up from 0. NumberFlow only animates when the value changes, so it mounts at 0
 * and switches to the real number on the next frame; later background refetches roll
 * from the old value to the new one.
 *
 * NumberFlow respects "reduce motion" by default.
 */
function AnimatedStat({ value, fractionDigits, suffix }: StatValue) {
  const [mounted, setMounted] = useState(false);

  // Switch on the frame after 0 is drawn, so NumberFlow has a 0 to animate from.
  useEffect(() => {
    const frame = requestAnimationFrame(() => setMounted(true));
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <NumberFlow
      format={{
        maximumFractionDigits: fractionDigits,
        minimumFractionDigits: fractionDigits,
      }}
      locales={getLocale()}
      suffix={suffix}
      value={mounted ? value : 0}
    />
  );
}
