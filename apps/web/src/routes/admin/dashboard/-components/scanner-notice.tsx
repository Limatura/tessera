import { IconShieldOff } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { formatNumber } from "@tessera/ui/lib/format";

import { useScanner } from "@/hooks/admin/use-scanner";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

/**
 * Shown only while a configured scanner is not answering. Uploads are served
 * unscanned meanwhile (the scanner fails open), and nothing else on the
 * dashboard would show it.
 *
 * Needs `settings:read`, so the caller mounts it only for accounts that hold it.
 */
export function ScannerNotice() {
  const scanner = useScanner();

  if (!scanner.data?.enabled || scanner.data.reachable) {
    return null;
  }

  return (
    <div
      className="flex gap-3 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-sm"
      role="alert"
    >
      <IconShieldOff className="mt-0.5 size-4 shrink-0 text-destructive" />
      <p>
        {m.dashboard_scanner_unreachable({
          deferred: formatNumber(scanner.data.deferred, getLocale()),
        })}{" "}
        <Link
          className="whitespace-nowrap underline underline-offset-4"
          to="/admin/settings/scan"
        >
          {m.dashboard_scanner_check()}
        </Link>
      </p>
    </div>
  );
}
