import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";

import * as m from "@/paraglide/messages";

type AdminShareStatus = components["schemas"]["AdminShareStatus"];

export function ShareStatusBadge({ status }: { status: AdminShareStatus }) {
  const { label, variant } = shareStatus(status);
  return <Badge variant={variant}>{label}</Badge>;
}

/**
 * 「已失效」不夠用:過期、次數用完是正常的生命週期,被掃到惡意軟體停用則是要
 * 有人處理的事,所以 `disabled` 另外標成 destructive。
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
