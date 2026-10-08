import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@tessera/ui/components/chart";
import type { ChartConfig } from "@tessera/ui/components/chart";
import { formatBytes } from "@tessera/ui/lib/format";
import { Bar, BarChart, LabelList, XAxis, YAxis } from "recharts";

import * as m from "@/paraglide/messages";

/** 內容放在哪裡：一個 backend，或資料庫本身。 */
export interface StorageLocation {
  name: string;
  usedBytes: number;
  /** `null` 是沒有上限。資料庫那一列永遠是 `null`。 */
  capacityBytes: number | null;
}

const ROW_HEIGHT = 36;
const BAR_SIZE = 18;

function buildChartConfig() {
  return {
    usedBytes: {
      color: "var(--primary)",
      label: m.dashboard_used_space(),
    },
  } satisfies ChartConfig;
}

/**
 * 每個存放位置用了多少，一條橫條一個位置。
 *
 * 比的是量，所以單一色相（`--primary`）就夠 —— 設計系統的 `--chart-*` 是一組灰階，
 * 拿來當分類色分不出來。哪個位置有狀況（連不上、快滿）由外面的警示列用圖示加文字說，
 * 不靠顏色。數字直接標在條尾，容量放在 tooltip。
 */
export function StorageLocationChart({
  locations,
}: {
  locations: StorageLocation[];
}) {
  const chartConfig = buildChartConfig();

  return (
    <ChartContainer
      className="aspect-auto w-full"
      config={chartConfig}
      style={{ height: locations.length * ROW_HEIGHT + 8 }}
    >
      <BarChart
        accessibilityLayer
        data={locations}
        layout="vertical"
        margin={{ left: 0, right: 80 }}
      >
        <XAxis dataKey="usedBytes" hide type="number" />
        <YAxis
          axisLine={false}
          dataKey="name"
          tickLine={false}
          type="category"
          width={96}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              formatter={(value, _name, item) =>
                `${formatBytes(Number(value))} / ${capacityLabel(item.payload as StorageLocation)}`
              }
              hideIndicator
            />
          }
          cursor={false}
        />
        <Bar
          barSize={BAR_SIZE}
          dataKey="usedBytes"
          fill="var(--color-usedBytes)"
          // 用量 0 的位置也要看得到，不然像是這個 backend 不存在。
          minPointSize={2}
          radius={4}
        >
          <LabelList
            className="fill-foreground"
            dataKey="usedBytes"
            fontSize={12}
            formatter={(value: unknown) => formatBytes(Number(value))}
            position="right"
          />
        </Bar>
      </BarChart>
    </ChartContainer>
  );
}

function capacityLabel(location: StorageLocation): string {
  return location.capacityBytes === null
    ? m.settings_storage_backends_capacity_unlimited()
    : formatBytes(location.capacityBytes);
}
