import { IconAlertTriangle, IconCircleCheck } from "@tabler/icons-react";
import { Skeleton } from "@tessera/ui/components/skeleton";
import { formatNumber } from "@tessera/ui/lib/format";

import { useScanner } from "@/hooks/admin/use-scanner";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";
import { SettingsCard } from "./settings-page";

/**
 * Whether the scanner answered the last time it was asked, and how much is
 * waiting on it. Hidden while `scan.url` is empty: the server then says nothing
 * about a scanner, and the form above already says scanning is off.
 *
 * A saved `scan.url` is not probed, so a wrong address shows up here as the
 * scanner not answering, not as an error on the form.
 */
export function ScannerStatusCard() {
  const scanner = useScanner();
  const formatDateTime = useDateTimeFormatter();
  const locale = getLocale();

  if (scanner.data && !scanner.data.enabled) {
    return null;
  }

  return (
    <SettingsCard
      description={m.settings_scanner_status_description()}
      title={m.settings_scanner_status_title()}
    >
      {scanner.isPending && <Skeleton className="h-16" />}
      {scanner.isError && (
        <p className="text-destructive text-sm" role="alert">
          {m.settings_scanner_status_load_error()}
        </p>
      )}
      {scanner.data && (
        <div className="flex flex-col gap-4">
          {scanner.data.reachable ? (
            <SettingsNotice
              icon={<IconCircleCheck />}
              title={m.settings_scanner_status_ok()}
            >
              {scanner.data.last_checked_at
                ? m.settings_scanner_status_last_checked({
                    time: formatDateTime(scanner.data.last_checked_at),
                  })
                : m.settings_scanner_status_not_checked()}
            </SettingsNotice>
          ) : (
            <SettingsNotice
              icon={<IconAlertTriangle />}
              title={m.settings_scanner_status_down()}
              tone="danger"
            >
              <p>{m.settings_scanner_status_down_body()}</p>
              {scanner.data.last_error && (
                <p className="mt-1 break-words font-mono text-xs">
                  {scanner.data.last_error}
                </p>
              )}
              {scanner.data.last_checked_at && (
                <p className="mt-1 text-xs">
                  {m.settings_scanner_status_last_checked({
                    time: formatDateTime(scanner.data.last_checked_at),
                  })}
                </p>
              )}
            </SettingsNotice>
          )}

          <dl className="grid grid-cols-2 gap-3 text-sm">
            <Count
              label={m.settings_scanner_status_pending()}
              value={formatNumber(scanner.data.pending, locale)}
            />
            <Count
              label={m.settings_scanner_status_deferred()}
              value={formatNumber(scanner.data.deferred, locale)}
            />
          </dl>
        </div>
      )}
    </SettingsCard>
  );
}

function Count({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5 rounded-lg border p-3">
      <dt className="text-muted-foreground text-xs">{label}</dt>
      <dd className="font-semibold text-lg tabular-nums">{value}</dd>
    </div>
  );
}
