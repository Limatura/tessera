import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";

import * as m from "@/paraglide/messages";

type AdminShare = components["schemas"]["AdminShareResponse"];
type AdminShareStatus = components["schemas"]["AdminShareStatus"];

export function ShareStatusBadge({
  share,
}: {
  share: Pick<AdminShare, "status" | "scan_state" | "scan_detail">;
}) {
  const { label, variant } = shareStatus(share.status);
  const threat = share.scan_state === "infected" ? share.scan_detail : null;
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <Badge variant={variant}>{label}</Badge>
      {/* The scanner's name for what it found, so the admin knows what was caught. */}
      {share.scan_state === "infected" && (
        <span className="font-mono text-destructive text-xs">
          {threat
            ? m.all_shares_scan_infected({ threat })
            : m.all_shares_scan_infected_unknown()}
        </span>
      )}
    </span>
  );
}

/**
 * "Inactive" is not enough: expiring or running out of downloads is the normal
 * lifecycle, but being disabled by the malware scanner needs someone to act, so
 * `disabled` is marked destructive.
 */
function shareStatus(status: AdminShareStatus): {
  label: string;
  variant: "secondary" | "outline" | "destructive";
} {
  switch (status) {
    case "available": {
      return { label: m.all_shares_status_available(), variant: "secondary" };
    }
    case "disabled": {
      return { label: m.all_shares_status_disabled(), variant: "destructive" };
    }
    case "expired": {
      return { label: m.all_shares_status_expired(), variant: "outline" };
    }
    case "exhausted": {
      return { label: m.all_shares_status_exhausted(), variant: "outline" };
    }
    default: {
      return { label: status, variant: "outline" };
    }
  }
}
