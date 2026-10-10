import { IconDatabase, IconServer } from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { ScrollArea } from "@tessera/ui/components/scroll-area";
import { formatBytes, formatNumber } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";

import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { UsageBar, usagePercent } from "./usage-bar";

/** 內容放在哪裡：一個 backend，或資料庫本身。 */
export interface StorageLocation {
  kind: "backend" | "database";
  name: string;
  usedBytes: number;
  /** `null` 是沒有上限。資料庫那一列永遠是 `null`。 */
  capacityBytes: number | null;
  /** 資料庫那一列沒有這些旗標。 */
  isDefault?: boolean;
  readOnly?: boolean;
  reachable?: boolean;
  directAccess?: boolean;
}

/**
 * 第一個位置不透明，之後一層層淡下去，但不淡到看不見。
 *
 * 單一色相（`--primary`）：設計系統的 `--chart-*` 是一組灰階，當不了分類色。顏色只
 * 是讓分段跟下面那張卡片的左側色帶對得起來，每張卡片都另外寫著名字跟數字，不靠顏色辨認。
 */
function segmentOpacity(index: number): number {
  return Math.max(1 - index * 0.15, 0.4);
}

/**
 * 每個存放位置：上面一條分段長條看「全站內容分佈在哪」，下面一個位置一張卡片看
 * 它自己的狀態和用量，一張一列。
 *
 * 卡片上的長條一定會畫，但比的東西看有沒有上限：有上限比它自己的容量（過 80% /
 * 95% 變色，同 `UsageBar`），沒有上限就比全站總量 —— 對「無限」的比例沒有意義。
 * 比的是哪一個，長條下面一定用文字寫出來，不讓人猜。
 */
export function StorageLocations({
  locations,
}: {
  locations: StorageLocation[];
}) {
  const total = locations.reduce(
    (sum, location) => sum + location.usedBytes,
    0
  );

  return (
    <div className="flex flex-col gap-4">
      {total > 0 && (
        // 純裝飾：每張卡片都寫著用量，螢幕閱讀器不必再聽一次比例。
        <div
          aria-hidden
          className="flex h-2 w-full gap-px overflow-hidden rounded-full bg-muted"
        >
          {locations.map((location, index) =>
            location.usedBytes > 0 ? (
              <div
                className="h-full bg-primary transition-[width]"
                key={location.name}
                style={{
                  opacity: segmentOpacity(index),
                  width: `${(location.usedBytes / total) * 100}%`,
                }}
              />
            ) : null
          )}
        </div>
      )}
      {/*
        位置一多，卡片會把整列撐高，連同一列的趨勢圖一起被拉長，所以清單自己捲。
        max-h 要設在 viewport 上：它是 size-full，外層沒有固定高度時百分比算不
        出來，設在 Root 上不會捲；設固定 h 又會讓只有兩個位置時多一塊空白。
        高度挑在卡片中段截斷（卡片 80px、間距 8px），露出半張讓人知道下面還有；
        截在卡片底部附近會看起來像排版壞掉。
      */}
      <ScrollArea className="**:data-[slot=scroll-area-viewport]:max-h-60">
        <ul className="flex flex-col gap-2 pe-3">
          {locations.map((location, index) => (
            <LocationCard
              index={index}
              key={location.name}
              location={location}
              total={total}
            />
          ))}
        </ul>
      </ScrollArea>
    </div>
  );
}

function LocationCard({
  location,
  index,
  total,
}: {
  location: StorageLocation;
  index: number;
  total: number;
}) {
  const locale = getLocale();
  const { capacityBytes } = location;
  const unreachable = location.reachable === false;
  const Icon = location.kind === "database" ? IconDatabase : IconServer;
  const sharePercent = total > 0 ? usagePercent(location.usedBytes, total) : 0;

  // 每張卡片的結構都一樣（標題列、長條、說明各一行），高度才會一致：徽章跟名稱
  // 同一行、不換行，擠不下時截的是名稱。
  return (
    <li
      className={cn(
        "relative flex items-start gap-3 overflow-hidden rounded-lg border bg-muted/30 py-3 ps-4 pe-3",
        unreachable && "border-destructive/50 bg-destructive/5"
      )}
    >
      {/* 左側色帶：對應上面分段長條裡它那一段。 */}
      <span
        aria-hidden
        className="absolute inset-y-0 start-0 w-0.5 bg-primary"
        style={{ opacity: segmentOpacity(index) }}
      />

      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted">
        <Icon className="size-4 text-muted-foreground" />
      </span>

      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate font-medium text-sm">{location.name}</span>
          <span className="flex shrink-0 gap-1">
            {location.isDefault && (
              <Badge variant="outline">
                {m.settings_storage_backends_default_badge()}
              </Badge>
            )}
            {unreachable && (
              <Badge variant="destructive">
                {m.settings_storage_backends_unreachable_badge()}
              </Badge>
            )}
            {location.readOnly && (
              <Badge variant="secondary">
                {m.settings_storage_backends_read_only_badge()}
              </Badge>
            )}
            {location.directAccess && (
              <Badge variant="secondary">
                {m.settings_storage_backends_direct_access_badge()}
              </Badge>
            )}
          </span>
          <span className="ms-auto shrink-0 text-muted-foreground text-xs tabular-nums">
            <span className="font-medium text-foreground text-sm">
              {formatBytes(location.usedBytes)}
            </span>
            {" / "}
            {capacityBytes === null
              ? m.settings_storage_backends_capacity_unlimited()
              : formatBytes(capacityBytes)}
          </span>
        </div>

        {capacityBytes === null ? (
          <div
            aria-hidden
            className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
          >
            <div
              className="h-full rounded-full bg-primary transition-[width]"
              style={{
                opacity: segmentOpacity(index),
                width: `${Math.min(sharePercent, 100)}%`,
              }}
            />
          </div>
        ) : (
          <UsageBar total={capacityBytes} used={location.usedBytes} />
        )}

        <span className="truncate text-muted-foreground text-xs">
          {capacityBytes === null
            ? m.dashboard_storage_location_of_total({
                percent: formatNumber(sharePercent, locale),
              })
            : m.dashboard_storage_location_of_capacity({
                percent: formatNumber(
                  usagePercent(location.usedBytes, capacityBytes),
                  locale
                ),
              })}
        </span>
      </div>
    </li>
  );
}
