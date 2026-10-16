import { Link } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { formatBytes, formatNumber } from "@tessera/ui/lib/format";

import { useSettings } from "@/hooks/admin/use-settings";
import { useStats } from "@/hooks/admin/use-stats";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

type Setting = components["schemas"]["SettingResponse"];
type SettingsRoute =
  | "/admin/settings/account"
  | "/admin/settings/features"
  | "/admin/settings/scan"
  | "/admin/settings/share"
  | "/admin/settings/upload";

interface Row {
  key: string;
  label: () => string;
  format: (value: unknown, context: FormatContext) => string;
  /** Which page edits it. `null` for keys the panel has no form for (environment only). */
  to: SettingsRoute | null;
}

interface FormatContext {
  /**
   * Invite codes still usable, listed when the registration policy is `invite`.
   * `undefined` if unavailable.
   */
  liveInvites: number | undefined;
}

/**
 * The values operators most often need to check.
 *
 * Values come from `GET /admin/settings` and are the ones in effect, whether saved in
 * the panel, set by environment or defaulted. "Restart required" is not hardcoded here
 * but read from the server's `refusal`: of the keys listed, only `server.max_body_bytes`
 * is `fixed_at_boot` today, and the rest can be changed live in the panel.
 */
const ROWS: Row[] = [
  {
    format: (value) => bytesOrUnlimited(value),
    key: "server.max_body_bytes",
    label: m.dashboard_config_max_body_bytes,
    to: "/admin/settings/upload",
  },
  {
    format: (value) => bytesOrUnlimited(value),
    key: "storage.max_total_bytes",
    label: m.dashboard_config_max_total_bytes,
    to: "/admin/settings/upload",
  },
  {
    format: onOff,
    key: "features.anonymous_upload",
    label: m.dashboard_config_anonymous_upload,
    to: "/admin/settings/upload",
  },
  {
    format: onOff,
    key: "features.accounts",
    label: m.dashboard_config_accounts,
    to: "/admin/settings/features",
  },
  {
    format: registrationLabel,
    key: "auth.registration",
    label: m.dashboard_config_registration_policy,
    to: "/admin/settings/account",
  },
  {
    format: (value) =>
      value === 0 ? m.dashboard_config_default_expiry_max() : hours(value),
    key: "share.default_expiry_hours",
    label: m.dashboard_config_default_expiry,
    to: "/admin/settings/upload",
  },
  {
    format: (value) =>
      value === 0 ? m.dashboard_config_retain_none() : hours(value),
    key: "share.retain_after_death_hours",
    label: m.dashboard_config_retain_after_death,
    to: "/admin/settings/share",
  },
  {
    // Empty means no scanning. The URL itself is not shown: it may be an
    // internal address, and the dashboard is not the place for it.
    format: (value) => onOff(typeof value === "string" && value.length > 0),
    key: "scan.url",
    label: m.dashboard_config_scan,
    to: "/admin/settings/scan",
  },
];

export function ConfigSummary() {
  const settings = useSettings();
  const stats = useStats();

  const byKey = new Map(
    settings.data?.settings.map((setting) => [setting.key, setting])
  );
  const context: FormatContext = { liveInvites: stats.data?.live_invites };

  return (
    <Card className="h-full ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_config_title()}</CardTitle>
        <CardDescription>{m.dashboard_config_description()}</CardDescription>
      </CardHeader>
      <CardContent>
        {settings.isPending && <Skeleton className="h-64" />}
        {settings.isError && (
          <p className="text-destructive text-sm">
            {m.dashboard_config_load_error()}
          </p>
        )}
        {settings.data && (
          <dl className="divide-y divide-border">
            {ROWS.map((row) => (
              <ConfigRow
                key={row.key}
                context={context}
                row={row}
                setting={byKey.get(row.key)}
              />
            ))}
          </dl>
        )}
      </CardContent>
    </Card>
  );
}

function ConfigRow({
  row,
  setting,
  context,
}: {
  row: Row;
  /** `undefined` when this server version lacks the key; the value then shows as a dash. */
  setting: Setting | undefined;
  context: FormatContext;
}) {
  const label = row.label();

  return (
    <div className="flex items-center justify-between gap-4 py-2.5 text-sm first:pt-0 last:pb-0">
      <dt className="text-muted-foreground">
        {row.to ? (
          <Link
            className="underline-offset-4 hover:text-foreground hover:underline"
            to={row.to}
          >
            {label}
          </Link>
        ) : (
          label
        )}
      </dt>
      <dd className="flex items-center gap-2 text-right font-medium tabular-nums">
        {setting?.refusal === "fixed_at_boot" && (
          <Badge variant="outline">{m.dashboard_config_fixed_badge()}</Badge>
        )}
        {setting
          ? row.format(setting.value, context)
          : m.common_dash_placeholder()}
      </dd>
    </div>
  );
}

function onOff(value: unknown): string {
  return value === true
    ? m.dashboard_config_enabled()
    : m.dashboard_config_disabled();
}

/** Both byte limits use 0 for "no limit". */
function bytesOrUnlimited(value: unknown): string {
  if (typeof value !== "number") {
    return m.common_dash_placeholder();
  }
  return value === 0 ? m.dashboard_config_unlimited() : formatBytes(value);
}

/** Whole days are written as days (168 → 7 days), easier to read than hours. */
function hours(value: unknown): string {
  if (typeof value !== "number") {
    return m.common_dash_placeholder();
  }
  const locale = getLocale();
  if (value > 0 && value % 24 === 0) {
    return m.dashboard_config_days({ days: formatNumber(value / 24, locale) });
  }
  return m.dashboard_config_hours({ hours: formatNumber(value, locale) });
}

function registrationLabel(value: unknown, context: FormatContext): string {
  switch (value) {
    case "open": {
      return m.dashboard_config_registration_open();
    }
    case "invite": {
      return context.liveInvites === undefined
        ? m.dashboard_config_registration_invite()
        : m.dashboard_config_registration_invite_count({
            count: formatNumber(context.liveInvites, getLocale()),
          });
    }
    case "closed": {
      return m.dashboard_config_registration_closed();
    }
    default: {
      return m.common_dash_placeholder();
    }
  }
}
