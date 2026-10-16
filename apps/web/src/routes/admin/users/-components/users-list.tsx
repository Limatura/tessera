import { Badge } from "@tessera/ui/components/badge";
import { cn } from "@tessera/ui/lib/utils";

import type { AdminUser } from "@/lib/admin/accounts";
import { isDeletedAccount } from "@/lib/admin/accounts";
import { accountRoleLabel, isUnassigned } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

import type { RowAbilities, RowAction } from "./user-row-parts";
import {
  FileLimitBadge,
  QuotaBadge,
  UserRowMenu,
  hasOwnLimits,
} from "./user-row-parts";

/**
 * The narrow-screen layout: one card per account. Name and state on top, since
 * that is what an admin scans for; email, role and limits below. The menu is
 * the same one as the table's, in the card's corner.
 */
export function UsersList({
  users,
  abilitiesFor,
  onAction,
}: {
  users: AdminUser[];
  abilitiesFor: ((user: AdminUser) => RowAbilities) | null;
  onAction: (user: AdminUser, action: RowAction) => void;
}) {
  return (
    <ul className="flex flex-col gap-3">
      {users.map((user) => (
        <UserCard
          abilities={abilitiesFor?.(user) ?? null}
          key={user.id}
          onAction={(action) => onAction(user, action)}
          user={user}
        />
      ))}
    </ul>
  );
}

function UserCard({
  user,
  abilities,
  onAction,
}: {
  user: AdminUser;
  abilities: RowAbilities | null;
  onAction: (action: RowAction) => void;
}) {
  const deleted = isDeletedAccount(user);
  const name = deleted ? m.users_deleted_account() : user.username;

  return (
    <li
      className={cn(
        "rounded-xl border bg-card",
        deleted && "border-dashed bg-muted/30"
      )}
    >
      <div className="flex items-start gap-3 p-4 pb-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <p
            className={cn(
              "flex items-center gap-2 font-medium text-sm",
              deleted && "text-muted-foreground italic"
            )}
          >
            <span className="truncate">{name}</span>
            {user.state === "disabled" && (
              <Badge variant="destructive">{m.users_state_disabled()}</Badge>
            )}
          </p>
          {!deleted && (
            <p className="truncate text-muted-foreground text-xs">
              {user.email}
            </p>
          )}
        </div>
        {abilities && (
          <div className="-mt-2 -mr-2 shrink-0">
            <UserRowMenu
              abilities={abilities}
              onAction={onAction}
              username={name}
            />
          </div>
        )}
      </div>

      {!deleted && (
        <div className="flex flex-wrap items-center gap-2 border-t px-4 py-3 text-xs">
          <Badge variant={isUnassigned(user) ? "outline" : "secondary"}>
            {accountRoleLabel(user)}
          </Badge>
          {isUnassigned(user) && (
            <span className="text-muted-foreground">
              {m.users_not_decided()}
            </span>
          )}
          {hasOwnLimits(user) && (
            <>
              <QuotaBadge quotaBytes={user.quota_bytes} />
              <FileLimitBadge maxFileBytes={user.max_file_bytes} />
            </>
          )}
          <span className="ml-auto text-muted-foreground tabular-nums">
            {m.users_permission_count({ count: user.permissions.length })}
          </span>
        </div>
      )}
    </li>
  );
}
