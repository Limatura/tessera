import { IconHistory } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import { buttonVariants } from "@tessera/ui/components/button";
import { cn } from "@tessera/ui/lib/utils";

import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

/** Whether the signed-in account may open the audit log. */
export function useCanReadAudit(): boolean {
  const { data: account } = useSession();
  return hasPermission(account, PERMISSIONS.auditRead);
}

/**
 * An icon link to the audit log, filtered to what was done to one share. Draws
 * nothing for an account without `audit:read`.
 */
export function ShareAuditLink({
  shareId,
  className,
}: {
  shareId: string;
  className?: string;
}) {
  if (!useCanReadAudit()) {
    return null;
  }
  return (
    <Link
      aria-label={m.audit_view_log()}
      className={cn(
        buttonVariants({ size: "icon", variant: "ghost" }),
        className
      )}
      search={{ target_id: shareId, target_type: "share" }}
      title={m.audit_view_log()}
      to="/admin/audit-log"
    >
      <IconHistory />
    </Link>
  );
}
