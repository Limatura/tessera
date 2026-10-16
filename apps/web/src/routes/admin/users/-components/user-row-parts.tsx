import {
  IconDots,
  IconGauge,
  IconPlayerPause,
  IconPlayerPlay,
  IconShieldCog,
  IconShieldOff,
  IconTrash,
  IconTrashX,
} from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@tessera/ui/components/dropdown-menu";
import { formatBytes } from "@tessera/ui/lib/format";

import * as m from "@/paraglide/messages";

import type { UserAction } from "./user-action-dialog";

/**
 * What the signed-in user may do to this row's account. With nothing allowed,
 * the menu is not shown.
 */
export interface RowAbilities {
  setRole: boolean;
  setQuota: boolean;
  disable: boolean;
  enable: boolean;
  removeShares: boolean;
  remove: boolean;
  /** Only where there is something to turn off (`mfa_enabled`). */
  resetMfa: boolean;
}

/**
 * Menu actions. The `UserAction` ones need confirmation; the rest open a dialog
 * or submit directly.
 */
export type RowAction = "enable" | "setQuota" | "setRole" | UserAction;

/** Whether the account has its own quota or file limit, so the column has anything to show. */
export function hasOwnLimits(user: {
  quota_bytes?: number | null;
  max_file_bytes?: number | null;
}): boolean {
  return (
    (user.quota_bytes !== null && user.quota_bytes !== undefined) ||
    (user.max_file_bytes !== null && user.max_file_bytes !== undefined)
  );
}

/**
 * Shown only when the account has its own quota. Unset (`null`) means the
 * role's, which the role column already says. `0` means unlimited for this
 * account; that differs from unset and must be visible.
 */
export function QuotaBadge({ quotaBytes }: { quotaBytes?: number | null }) {
  if (quotaBytes === null || quotaBytes === undefined) {
    return null;
  }
  return (
    <Badge variant="outline">
      {quotaBytes === 0
        ? m.users_quota_badge_unlimited()
        : m.users_quota_badge({ amount: formatBytes(quotaBytes) })}
    </Badge>
  );
}

export function FileLimitBadge({
  maxFileBytes,
}: {
  maxFileBytes?: number | null;
}) {
  if (maxFileBytes === null || maxFileBytes === undefined) {
    return null;
  }
  return (
    <Badge variant="outline">
      {maxFileBytes === 0
        ? m.users_file_badge_unlimited()
        : m.users_file_badge({ amount: formatBytes(maxFileBytes) })}
    </Badge>
  );
}

export function UserRowMenu({
  username,
  abilities,
  onAction,
}: {
  username: string;
  abilities: RowAbilities;
  onAction: (action: RowAction) => void;
}) {
  const { setRole, setQuota, disable, enable, resetMfa, removeShares, remove } =
    abilities;
  const hasManage = setRole || setQuota || disable || enable || resetMfa;
  if (!(hasManage || removeShares || remove)) {
    return null;
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={m.users_row_menu_aria({ username })}
            className="max-md:size-10"
            size="icon"
            type="button"
            variant="ghost"
          />
        }
      >
        <IconDots />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-44">
        {setRole && (
          <DropdownMenuItem onClick={() => onAction("setRole")}>
            <IconShieldCog />
            {m.users_menu_set_role()}
          </DropdownMenuItem>
        )}
        {setQuota && (
          <DropdownMenuItem onClick={() => onAction("setQuota")}>
            <IconGauge />
            {m.users_menu_set_quota()}
          </DropdownMenuItem>
        )}
        {disable && (
          <DropdownMenuItem onClick={() => onAction("disable")}>
            <IconPlayerPause />
            {m.users_menu_disable()}
          </DropdownMenuItem>
        )}
        {enable && (
          <DropdownMenuItem onClick={() => onAction("enable")}>
            <IconPlayerPlay />
            {m.users_menu_enable()}
          </DropdownMenuItem>
        )}
        {resetMfa && (
          <DropdownMenuItem onClick={() => onAction("resetMfa")}>
            <IconShieldOff />
            {m.users_menu_reset_mfa()}
          </DropdownMenuItem>
        )}
        {(removeShares || remove) && hasManage && <DropdownMenuSeparator />}
        {removeShares && (
          <DropdownMenuItem
            onClick={() => onAction("removeShares")}
            variant="destructive"
          >
            <IconTrashX />
            {m.users_menu_remove_shares()}
          </DropdownMenuItem>
        )}
        {remove && (
          <DropdownMenuItem
            onClick={() => onAction("delete")}
            variant="destructive"
          >
            <IconTrash />
            {m.users_menu_delete()}
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
