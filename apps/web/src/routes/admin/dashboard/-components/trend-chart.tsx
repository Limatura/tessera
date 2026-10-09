import { IconChartAreaLine } from "@tabler/icons-react";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
} from "@tessera/ui/components/chart";
import type { ChartConfig } from "@tessera/ui/components/chart";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { Tabs, TabsList, TabsTrigger } from "@tessera/ui/components/tabs";
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@tessera/ui/components/toggle-group";
import { formatBytes, formatNumber } from "@tessera/ui/lib/format";
import { useReducedMotion } from "motion/react";
import { useState } from "react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ReferenceLine,
  XAxis,
  YAxis,
} from "recharts";

import { useStats, useStatsHistory } from "@/hooks/admin/use-stats";
import { CHART_ANIMATION } from "@/lib/admin/motion";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { buildTrendPoints, trendChange } from "./trend-series";
import type { TrendKey } from "./trend-series";

const RANGES = [7, 30, 90] as const;
type Range = (typeof RANGES)[number];
const DEFAULT_RANGE: Range = 30;

type Metric = "storage" | "shares" | "traffic";
const METRICS: Metric[] = ["storage", "shares", "traffic"];

interface MetricSpec {
  label: () => string;
  description: () => string;
  /** 第一條是主角：用 `--primary`、有漸層、頁尾的「增加多少」看它。 */
  series: { key: TrendKey; label: () => string }[];
  format: (value: number) => string;
}

/**
 * 三種指標各自一張圖，同一時間只畫一種：單位不同（位元組 vs 筆數），擠在一張就得用
 * 兩個縱軸，那是最容易讀錯的圖。
 */
const METRIC_SPECS: Record<Metric, MetricSpec> = {
  shares: {
    description: m.dashboard_trend_description_shares,
    format: (value) => formatNumber(value, getLocale()),
    label: m.dashboard_trend_metric_shares,
    // 可取件是主角，總數當背景：兩條的差就是清掃還沒處理到的量。
    series: [
      { key: "liveShares", label: m.dashboard_trend_series_live_shares },
      { key: "shares", label: m.dashboard_trend_series_shares },
    ],
  },
  storage: {
    description: m.dashboard_trend_description_storage,
    format: formatBytes,
    label: m.dashboard_trend_metric_storage,
    series: [{ key: "storedBytes", label: m.dashboard_used_space }],
  },
  traffic: {
    description: m.dashboard_trend_description_traffic,
    format: formatBytes,
    label: m.dashboard_trend_metric_traffic,
    series: [{ key: "bytesServed", label: m.dashboard_trend_series_served }],
  },
};

/** 少於兩天畫不出「趨勢」，一個點只是現在的數字，上面的統計卡已經有了。 */
const MIN_RECORDED_DAYS = 2;

/**
 * 全站數字隨時間的變化，資料來自 `GET /admin/stats/history`（#410）。
 *
 * 每天一筆，是那天最後一次清掃時的值；歷史從這版部署那天才開始記。
 */
export function TrendChart() {
  const [range, setRange] = useState<Range>(DEFAULT_RANGE);
  const [metric, setMetric] = useState<Metric>("storage");
  const history = useStatsHistory(range);
  const spec = METRIC_SPECS[metric];

  const handleRangeChange = (values: unknown[]) => {
    const next = RANGES.find((option) => String(option) === values.at(0));
    // 再點一次已選的那顆會變成「什麼都沒選」，那種時候維持原狀。
    if (next) {
      setRange(next);
    }
  };

  const handleMetricChange = (value: unknown) => {
    const next = METRICS.find((option) => option === value);
    if (next) {
      setMetric(next);
    }
  };

  return (
    <Card className="h-full ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_trend_title()}</CardTitle>
        <CardDescription>{spec.description()}</CardDescription>
        <CardAction>
          <ToggleGroup
            aria-label={m.dashboard_trend_range_label()}
            onValueChange={handleRangeChange}
            size="sm"
            value={[String(range)]}
            variant="outline"
          >
            {RANGES.map((option) => (
              <ToggleGroupItem key={option} value={String(option)}>
                {m.dashboard_trend_range_days({ days: option })}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-4">
        <Tabs onValueChange={handleMetricChange} value={metric}>
          <TabsList aria-label={m.dashboard_trend_metric_label()}>
            {METRICS.map((option) => (
              <TabsTrigger key={option} value={option}>
                {METRIC_SPECS[option].label()}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        {history.isPending && <Skeleton className="min-h-64 flex-1" />}
        {history.isError && (
          <p className="text-destructive text-sm">
            {m.dashboard_trend_load_error()}
          </p>
        )}
        {history.data &&
          (history.data.data.length < MIN_RECORDED_DAYS ? (
            <EmptyTrend />
          ) : (
            <TrendPlot days={history.data.data} metric={metric} />
          ))}
      </CardContent>
    </Card>
  );
}

function TrendPlot({
  days,
  metric,
}: {
  days: Parameters<typeof buildTrendPoints>[0];
  metric: Metric;
}) {
  const quotaBytes = useStats().data?.quota_bytes ?? null;
  const reduceMotion = useReducedMotion();
  const spec = METRIC_SPECS[metric];
  const { points, hasGaps } = buildTrendPoints(days);
  const [lead] = spec.series;
  const change = lead ? trendChange(points, lead.key) : null;

  // 主角用強調色，其餘當背景用灰。設計系統的 `--chart-*` 是一組灰階，當不了分類色。
  const chartConfig: ChartConfig = Object.fromEntries(
    spec.series.map((series, index) => [
      series.key,
      {
        color: index === 0 ? "var(--primary)" : "var(--muted-foreground)",
        label: series.label(),
      },
    ])
  );

  // 日期是 UTC 的「那一天」，用 UTC 格式化，否則西半球的人會看到前一天。
  const dayFormatter = new Intl.DateTimeFormat(getLocale(), {
    day: "numeric",
    month: "numeric",
    timeZone: "UTC",
  });
  const formatDay = (value: string) =>
    dayFormatter.format(new Date(`${value}T00:00:00Z`));

  return (
    <>
      <ChartContainer
        className="aspect-auto min-h-64 flex-1"
        config={chartConfig}
      >
        <AreaChart data={points} margin={{ left: 8, right: 16, top: 16 }}>
          <defs>
            <linearGradient id="fillTrendLead" x1="0" x2="0" y1="0" y2="1">
              <stop offset="5%" stopColor="var(--primary)" stopOpacity={0.3} />
              <stop offset="95%" stopColor="var(--primary)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid vertical={false} />
          <XAxis
            axisLine={false}
            dataKey="date"
            minTickGap={24}
            tickFormatter={formatDay}
            tickLine={false}
            tickMargin={8}
          />
          <YAxis
            axisLine={false}
            tickFormatter={(value: number) => spec.format(value)}
            tickLine={false}
            width={80}
          />
          {metric === "storage" && quotaBytes !== null && (
            <ReferenceLine
              label={{
                fill: "var(--muted-foreground)",
                fontSize: 11,
                position: "insideTopRight",
                value: m.dashboard_trend_quota_line(),
              }}
              stroke="var(--muted-foreground)"
              strokeDasharray="4 4"
              y={quotaBytes}
            />
          )}
          <ChartTooltip
            content={
              <ChartTooltipContent
                formatter={(value, name) =>
                  `${chartConfig[String(name)]?.label ?? name}  ${spec.format(Number(value))}`
                }
                labelFormatter={(_label, payload) =>
                  formatDay(String(payload.at(0)?.payload?.date ?? ""))
                }
              />
            }
          />
          {spec.series.length > 1 && (
            <ChartLegend content={<ChartLegendContent />} />
          )}
          {/* 背景系列先畫，主角疊在最上面。 */}
          {spec.series.toReversed().map((series) => (
            <Area
              key={series.key}
              connectNulls={false}
              dataKey={series.key}
              fill={series.key === lead?.key ? "url(#fillTrendLead)" : "none"}
              {...CHART_ANIMATION}
              isAnimationActive={!reduceMotion}
              stroke={`var(--color-${series.key})`}
              strokeDasharray={series.key === lead?.key ? undefined : "4 4"}
              strokeWidth={2}
              type="monotone"
            />
          ))}
        </AreaChart>
      </ChartContainer>
      <CardFooter className="flex-col items-start gap-1 px-0 text-muted-foreground text-sm">
        {change && (
          <p>
            {change.delta >= 0
              ? m.dashboard_trend_grew({
                  amount: spec.format(change.delta),
                  days: change.days,
                })
              : m.dashboard_trend_shrank({
                  amount: spec.format(-change.delta),
                  days: change.days,
                })}
          </p>
        )}
        {hasGaps && <p className="text-xs">{m.dashboard_trend_gap_note()}</p>}
      </CardFooter>
    </>
  );
}

function EmptyTrend() {
  return (
    <div className="flex min-h-64 flex-1 flex-col items-center justify-center gap-2 rounded-md border border-dashed px-6 text-center">
      <IconChartAreaLine className="size-6 text-muted-foreground" />
      <p className="font-medium text-sm">{m.dashboard_trend_empty_title()}</p>
      <p className="max-w-sm text-muted-foreground text-xs">
        {m.dashboard_trend_empty_description()}
      </p>
    </div>
  );
}
