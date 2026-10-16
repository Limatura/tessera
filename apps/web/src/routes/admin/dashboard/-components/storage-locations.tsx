import { IconDatabase, IconServer } from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { ScrollArea } from "@tessera/ui/components/scroll-area";
import { formatBytes, formatNumber } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";

import {
  SegmentedUsageBar,
  segmentOpacity,
  UsageBar,
  usagePercent,
} from "@/components/admin/usage-bar";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

/** Where content lives: a backend, or the database itself. */
export interface StorageLocation {
  kind: "backend" | "database";
  name: string;
  usedBytes: number;
  /** `null` means no limit. Always `null` for the database row. */
  capacityBytes: number | null;
  /** The database row has none of these flags. */
  isDefault?: boolean;
  readOnly?: boolean;
  reachable?: boolean;
  directAccess?: boolean;
}

/**
 * Every storage location: a segmented bar on top for where the instance's content
 * lives, and below it one card per location, one per row, with its own state and usage.
 *
 * The card's bar is always drawn, but what it compares depends on the limit: its own
 * capacity when there is one (changing color past 80% / 95%, as `UsageBar` does),
 * otherwise the instance total, since a ratio of "unlimited" means nothing. The text
 * under the bar always says which, so it is never a guess.
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
        // Decorative only: every card states its usage, so screen readers need not hear the ratio.
        <SegmentedUsageBar segments={locations} />
      )}
      {/*
        With many locations the cards would make the row tall and stretch the trend chart
        beside it, so the list scrolls by itself. max-h goes on the viewport: it is
        size-full, and with no fixed height above it a percentage cannot resolve, so on
        Root it would not scroll; a fixed h would leave a gap with only two locations.
        The height cuts mid-card (80px cards, 8px gaps) so half a card hints at more;
        cutting near a card's bottom edge looks like a broken layout.
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

  // Every card has the same structure (title row, bar, caption, one line each) so they
  // are the same height: badge and name share a line without wrapping, and the name is
  // what gets truncated.
  return (
    <li
      className={cn(
        "relative flex items-start gap-3 overflow-hidden rounded-lg border bg-muted/30 py-3 ps-4 pe-3",
        unreachable && "border-destructive/50 bg-destructive/5"
      )}
    >
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
