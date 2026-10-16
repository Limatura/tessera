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
import { formatBytes } from "@tessera/ui/lib/format";
import type { ReactNode } from "react";

import {
  UsageBar,
  usagePercent,
  WARN_PERCENT,
} from "@/components/admin/usage-bar";
import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import { useStats } from "@/hooks/admin/use-stats";
import { useStorageBackends } from "@/hooks/admin/use-storage-backends";
import * as m from "@/paraglide/messages";

import { StorageLocations } from "./storage-locations";
import type { StorageLocation } from "./storage-locations";

type Backend = components["schemas"]["BackendResponse"];

/**
 * Total content against the total limit, plus where the content actually lives.
 *
 * The breakdown needs `/admin/storage/backends` (`settings:read`) while the dashboard
 * itself needs only `stats:read`; without that permission only the total is drawn.
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
 * No `storage.max_total_bytes` with guest uploads on (both defaults) is an unlimited
 * network drive for anyone who can reach the instance, and the first sign is writes
 * failing on a full disk. A missing limit alone needs no warning: instances where only
 * members upload are normal.
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
          <StorageLocations
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
 * Each backend, plus the database.
 *
 * Text and files under `share.inline_threshold_bytes` are stored in the database, on
 * no backend, but count toward the total. No endpoint reports that figure, so it is
 * derived as total minus backends. Uploads in progress are already on a backend but
 * not yet in the total, which can make it negative; it is then treated as 0 and not
 * drawn.
 */
function buildLocations(
  backends: Backend[],
  storedBytes: number | undefined
): StorageLocation[] {
  const locations: StorageLocation[] = backends.map((backend) => ({
    capacityBytes: backend.capacity_bytes ?? null,
    directAccess: backend.direct_access,
    isDefault: backend.is_default,
    kind: "backend",
    name: backend.name,
    reachable: backend.reachable,
    readOnly: backend.read_only,
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
      kind: "database",
      name: m.dashboard_storage_database(),
      usedBytes: inDatabase,
    });
  }
  return locations;
}

/**
 * Unreachable backends first: their shares all answer 503 while everything else looks
 * normal, so without a flag no one notices. Nearly full ones next.
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
