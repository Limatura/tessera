import { Badge } from "@tessera/ui/components/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";

import { formatBytes } from "@/lib/format";
import * as m from "@/paraglide/messages";

/**
 * Mirrors the server's `Capabilities` response (apps/server/src/api/capabilities.rs)
 * plus a couple of admin-only fields from `Config`. Static placeholder values
 * until GET /capabilities (and an admin-only config readout) are wired up.
 *
 * Everything here is set via TESSERA_* env vars / config file at process
 * startup — none of it is editable through this UI.
 */
const config = {
  accounts: true,
  anonymousUpload: true,
  maxBodyBytes: 104_857_600,
  registration: "open" as const,
  retainAfterDeathHours: 168,
  storageUrl: "data/blobs",
};

function buildRows(): { label: string; value: string }[] {
  return [
    { label: m.dashboard_config_storage_backend(), value: config.storageUrl },
    {
      label: m.dashboard_config_max_body_bytes(),
      value: formatBytes(config.maxBodyBytes),
    },
    {
      label: m.dashboard_config_anonymous_upload(),
      value: config.anonymousUpload
        ? m.dashboard_config_enabled()
        : m.dashboard_config_disabled(),
    },
    {
      label: m.dashboard_config_accounts(),
      value: config.accounts
        ? m.dashboard_config_enabled()
        : m.dashboard_config_disabled(),
    },
    {
      label: m.dashboard_config_registration_policy(),
      value: registrationLabel(config.registration),
    },
    {
      label: m.dashboard_config_retain_after_death(),
      value: m.dashboard_config_retain_hours({
        hours: config.retainAfterDeathHours,
      }),
    },
  ];
}

export function ConfigSummary() {
  const rows = buildRows();

  return (
    <Card className="ring-foreground/10">
      <CardHeader>
        <CardTitle>{m.dashboard_config_title()}</CardTitle>
        <CardDescription>{m.dashboard_config_description()}</CardDescription>
      </CardHeader>
      <CardContent>
        <dl className="divide-y divide-border">
          {rows.map((row) => (
            <div
              key={row.label}
              className="flex items-center justify-between py-2.5 text-sm first:pt-0 last:pb-0"
            >
              <dt className="text-muted-foreground">{row.label}</dt>
              <dd className="font-medium tabular-nums">{row.value}</dd>
            </div>
          ))}
        </dl>
        <Badge variant="outline" className="mt-3">
          {m.dashboard_config_readonly_badge()}
        </Badge>
      </CardContent>
    </Card>
  );
}

function registrationLabel(registration: typeof config.registration) {
  if (registration === "open") {
    return m.dashboard_config_registration_open();
  }
  if (registration === "invite") {
    return m.dashboard_config_registration_invite();
  }
  return m.dashboard_config_disabled();
}
