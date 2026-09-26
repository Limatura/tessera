import { IconTrendingUp } from "@tabler/icons-react";
import {
  Card,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@tessera/ui/components/chart";
import type { ChartConfig } from "@tessera/ui/components/chart";
import { Area, AreaChart, CartesianGrid, XAxis } from "recharts";

import mockData from "@/data/dashboard-mock.json";
import { formatBytes } from "@/lib/format";
import * as m from "@/paraglide/messages";

/**
 * Placeholder time series until GET /admin/stats carries history — today the
 * endpoint returns only a live snapshot, so this chart has nothing real to
 * plot yet. Backed by static mock data, not wired up.
 */
const data = mockData.storageTrend;

function buildChartConfig() {
  return {
    bytes: {
      color: "var(--primary)",
      label: m.dashboard_storage_trend_used_space(),
    },
  } satisfies ChartConfig;
}

const first = data.at(0)?.bytes ?? 0;
const last = data.at(-1)?.bytes ?? 0;
const growthPercent = first > 0 ? ((last - first) / first) * 100 : 0;

export function StorageTrendChart() {
  const chartConfig = buildChartConfig();

  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_storage_trend_title()}</CardTitle>
        <CardDescription>
          {m.dashboard_storage_trend_description()}
        </CardDescription>
      </CardHeader>
      <ChartContainer config={chartConfig} className="aspect-auto h-56 px-2">
        <AreaChart data={data} margin={{ left: 0, right: 12 }}>
          <defs>
            <linearGradient id="fillBytes" x1="0" y1="0" x2="0" y2="1">
              <stop
                offset="5%"
                stopColor="var(--color-bytes)"
                stopOpacity={0.35}
              />
              <stop
                offset="95%"
                stopColor="var(--color-bytes)"
                stopOpacity={0}
              />
            </linearGradient>
          </defs>
          <CartesianGrid vertical={false} />
          <XAxis
            dataKey="day"
            tickLine={false}
            axisLine={false}
            tickMargin={8}
          />
          <ChartTooltip
            content={
              <ChartTooltipContent
                formatter={(value) => formatBytes(Number(value))}
              />
            }
          />
          <Area
            dataKey="bytes"
            type="monotone"
            fill="url(#fillBytes)"
            stroke="var(--color-bytes)"
          />
        </AreaChart>
      </ChartContainer>
      <CardFooter className="flex-col items-start gap-1.5 text-sm">
        <div className="flex items-center gap-2 font-medium">
          <IconTrendingUp className="size-4 text-primary" />
          {m.dashboard_storage_trend_growth({
            percent: growthPercent.toFixed(1),
          })}
        </div>
        <div className="text-muted-foreground">
          {m.dashboard_storage_trend_current_used({
            amount: formatBytes(last),
          })}
        </div>
      </CardFooter>
    </Card>
  );
}
