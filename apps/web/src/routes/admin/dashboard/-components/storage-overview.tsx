import { IconAlertTriangle, IconPlugConnectedX } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Skeleton } from "@tessera/ui/components/skeleton";
import type { ReactNode } from "react";

import { useStats, useStorageBackends } from "@/hooks/admin/use-admin";
import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import { formatBytes } from "@/lib/format";
import * as m from "@/paraglide/messages";

import { StorageLocationChart } from "./storage-location-chart";
import type { StorageLocation } from "./storage-location-chart";
import { UsageBar, usagePercent, WARN_PERCENT } from "./usage-bar";

type Backend = components["schemas"]["BackendResponse"];

/**
 * 全站內容總量對總量上限，加上內容實際放在哪裡。
 *
 * 各位置的分佈要 `/admin/storage/backends`（`settings:read`），儀表板本身只要
 * `stats:read`，所以沒有那個權限時只畫總量。
 */
export function StorageOverview({
  canReadSettings,
}: {
  canReadSettings: boolean;
}) {
  return (
    <Card className="h-full ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_storage_title()}</CardTitle>
        <CardDescription>{m.dashboard_storage_description()}</CardDescription>
        {canReadSettings && (
          <CardAction>
            <Link
              className={buttonVariants({ size: "sm", variant: "ghost" })}
              to="/admin/settings/storage"
            >
              {m.dashboard_storage_manage()}
            </Link>
          </CardAction>
        )}
      </CardHeader>
      <CardContent className="flex flex-col gap-6">
        <TotalUsage canReadSettings={canReadSettings} />
        {canReadSettings && <Locations />}
      </CardContent>
    </Card>
  );
}

function TotalUsage({ canReadSettings }: { canReadSettings: boolean }) {
  const stats = useStats();

  if (!stats.data) {
    return <Skeleton className="h-10" />;
  }

  const { stored_bytes: stored, quota_bytes: quota } = stats.data;
  const hasQuota = quota !== null && quota !== undefined;

  return (
    <section className="flex flex-col gap-2">
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 text-sm">
        <span className="font-medium">{m.dashboard_storage_total()}</span>
        <span className="text-muted-foreground tabular-nums">
          <span className="font-medium text-foreground">
            {formatBytes(stored)}
          </span>
          {" / "}
          {hasQuota
            ? formatBytes(quota)
            : m.settings_storage_backends_capacity_unlimited()}
        </span>
      </div>
      {hasQuota && <UsageBar total={quota} used={stored} />}
      {!hasQuota && canReadSettings && <NoQuotaWarning />}
    </section>
  );
}

/**
 * 沒設 `storage.max_total_bytes` 又開著訪客上傳（兩個都是預設值）= 對所有連得到的人
 * 開放的無限網路硬碟，第一個徵兆是磁碟滿了寫入開始失敗。只有沒設上限不必警告 ——
 * 全是自己人上傳的站台很正常。
 */
function NoQuotaWarning() {
  const settings = useSettings();

  if (readSetting(settings.data, "features.anonymous_upload") !== true) {
    return null;
  }

  return (
    <Notice tone="warning">
      {m.dashboard_storage_no_quota_warning()}{" "}
      <Link
        className="whitespace-nowrap underline underline-offset-4"
        to="/admin/settings/upload"
      >
        {m.dashboard_storage_set_quota()}
      </Link>
    </Notice>
  );
}

function Locations() {
  const backends = useStorageBackends();
  const stats = useStats();

  return (
    <section className="flex flex-col gap-3">
      <h3 className="font-medium text-muted-foreground text-xs">
        {m.dashboard_storage_locations()}
      </h3>
      {backends.isPending && <Skeleton className="h-24" />}
      {backends.isError && (
        <p className="text-destructive text-sm">
          {m.settings_storage_backends_load_error()}
        </p>
      )}
      {backends.data && (
        <>
          <BackendNotices backends={backends.data} />
          <StorageLocationChart
            locations={buildLocations(backends.data, stats.data?.stored_bytes)}
          />
          <p className="text-muted-foreground text-xs">
            {m.dashboard_storage_backends_note()}
          </p>
        </>
      )}
    </section>
  );
}

/**
 * 各 backend，再加上資料庫。
 *
 * 文字和小於 `share.inline_threshold_bytes` 的檔案存在資料庫，不在任何 backend 上，
 * 全站總量卻有算它們。沒有端點直接回報這個數字，用「總量 − 各 backend」推出來：
 * 進行中的上傳已寫進 backend 卻還沒算進總量，所以可能推出負數，那時當 0，不畫這一條。
 */
function buildLocations(
  backends: Backend[],
  storedBytes: number | undefined
): StorageLocation[] {
  const locations: StorageLocation[] = backends.map((backend) => ({
    capacityBytes: backend.capacity_bytes ?? null,
    name: backend.name,
    usedBytes: backend.used_bytes,
  }));
  if (storedBytes === undefined) {
    return locations;
  }
  const onBackends = backends.reduce(
    (sum, backend) => sum + backend.used_bytes,
    0
  );
  const inDatabase = storedBytes - onBackends;
  if (inDatabase > 0) {
    locations.push({
      capacityBytes: null,
      name: m.dashboard_storage_database(),
      usedBytes: inDatabase,
    });
  }
  return locations;
}

/**
 * 連不上的 backend 最要緊：上面的分享全都回 503，而其他欄位看起來都正常，不標出來
 * 就沒人會發現。其次是快滿的。
 */
function BackendNotices({ backends }: { backends: Backend[] }) {
  const unreachable = backends.filter((backend) => !backend.reachable);
  const nearlyFull = backends.filter(
    (backend) =>
      backend.capacity_bytes !== null &&
      backend.capacity_bytes !== undefined &&
      usagePercent(backend.used_bytes, backend.capacity_bytes) >= WARN_PERCENT
  );

  if (unreachable.length === 0 && nearlyFull.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-col gap-2">
      {unreachable.map((backend) => (
        <Notice key={`unreachable-${backend.name}`} tone="critical">
          {m.dashboard_storage_backend_unreachable({ name: backend.name })}
        </Notice>
      ))}
      {nearlyFull.map((backend) => (
        <Notice key={`full-${backend.name}`} tone="warning">
          {m.dashboard_storage_backend_nearly_full({
            name: backend.name,
            percent: usagePercent(
              backend.used_bytes,
              backend.capacity_bytes ?? 0
            ),
          })}
        </Notice>
      ))}
    </div>
  );
}

function Notice({
  tone,
  children,
}: {
  tone: "warning" | "critical";
  children: ReactNode;
}) {
  const Icon = tone === "critical" ? IconPlugConnectedX : IconAlertTriangle;

  return (
    <div
      className={
        tone === "critical"
          ? "flex gap-3 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm"
          : "flex gap-3 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-sm"
      }
      role="alert"
    >
      <Icon
        className={
          tone === "critical"
            ? "mt-0.5 size-4 shrink-0 text-destructive"
            : "mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-500"
        }
      />
      <p>{children}</p>
    </div>
  );
}
