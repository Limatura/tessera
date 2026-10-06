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

import { useStats } from "@/hooks/admin/use-admin";
import { useSettings } from "@/hooks/admin/use-settings";
import { formatBytes, formatNumber } from "@/lib/format";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

type Setting = components["schemas"]["SettingResponse"];
type SettingsRoute =
  | "/admin/settings/account"
  | "/admin/settings/features"
  | "/admin/settings/share"
  | "/admin/settings/upload";

interface Row {
  key: string;
  label: () => string;
  format: (value: unknown, context: FormatContext) => string;
  /** 在哪一頁改。`null` 是面板沒有表單的 key（例如 `scan.*`，只能用環境變數）。 */
  to: SettingsRoute | null;
}

interface FormatContext {
  /** 還能用的邀請碼數，註冊政策是 `invite` 時一起列。拿不到就是 `undefined`。 */
  liveInvites: number | undefined;
}

/**
 * 營運上最常被問「現在到底是怎樣」的幾個值。
 *
 * 值來自 `GET /admin/settings`，是實際生效的那一份 —— 不管是面板存的、環境變數給的
 * 還是預設值。「需重啟」不寫死在這裡，看 server 回的 `refusal`：現在只有
 * `server.max_body_bytes` 是 `fixed_at_boot`，其他都能在後台直接改。
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
    // 空字串是不掃。URL 本身不秀：可能是內網位址，儀表板不是放它的地方。
    format: (value) => onOff(typeof value === "string" && value.length > 0),
    key: "scan.url",
    label: m.dashboard_config_scan,
    to: null,
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
  /** 這版 server 沒有這個 key 時是 `undefined`，值就顯示成破折號。 */
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

/** 兩個 byte 上限都用 0 表示「不限」。 */
function bytesOrUnlimited(value: unknown): string {
  if (typeof value !== "number") {
    return m.common_dash_placeholder();
  }
  return value === 0 ? m.dashboard_config_unlimited() : formatBytes(value);
}

/** 整天數就寫天（168 → 7 天），比一串小時好讀。 */
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
