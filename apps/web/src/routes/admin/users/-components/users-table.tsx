import { Badge } from "@tessera/ui/components/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";

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

/** The `md` and up layout; `UsersList` is the phone one. */
export function UsersTable({
  users,
  abilitiesFor,
  onAction,
}: {
  users: AdminUser[];
  /** `null` when the signed-in user can do nothing, which hides the actions column. */
  abilitiesFor: ((user: AdminUser) => RowAbilities) | null;
  onAction: (user: AdminUser, action: RowAction) => void;
}) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{m.users_col_account()}</TableHead>
          <TableHead>{m.users_col_email()}</TableHead>
          <TableHead>{m.users_col_role()}</TableHead>
          <TableHead>{m.users_col_limits()}</TableHead>
          <TableHead className="text-right">
            {m.users_col_permission_count()}
          </TableHead>
          {abilitiesFor && <TableHead className="w-12" />}
        </TableRow>
      </TableHeader>
      <TableBody>
        {users.map((user) => {
          const menu = abilitiesFor && (
            <TableCell>
              <UserRowMenu
                abilities={abilitiesFor(user)}
                onAction={(action) => onAction(user, action)}
                username={
                  isDeletedAccount(user)
                    ? m.users_deleted_account()
                    : user.username
                }
              />
            </TableCell>
          );
          return isDeletedAccount(user) ? (
            <DeletedUserRow key={user.id} menu={menu} />
          ) : (
            <TableRow key={user.id}>
              <TableCell className="font-medium">
                {user.username}
                {user.state === "disabled" && (
                  <Badge className="ml-2" variant="destructive">
                    {m.users_state_disabled()}
                  </Badge>
                )}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {user.email}
              </TableCell>
              <TableCell>
                <Badge variant={isUnassigned(user) ? "outline" : "secondary"}>
                  {accountRoleLabel(user)}
                </Badge>
                {isUnassigned(user) && (
                  <span className="ml-2 text-xs text-muted-foreground">
                    {m.users_not_decided()}
                  </span>
                )}
              </TableCell>
              <TableCell>
                {hasOwnLimits(user) ? (
                  <span className="flex flex-wrap gap-1">
                    <QuotaBadge quotaBytes={user.quota_bytes} />
                    <FileLimitBadge maxFileBytes={user.max_file_bytes} />
                  </span>
                ) : (
                  <span className="text-muted-foreground text-xs">
                    {m.users_limits_from_role()}
                  </span>
                )}
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {user.permissions.length}
              </TableCell>
              {menu}
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}

/** The tombstone's `gone-01K…` name means nothing to a person; say what it is instead. */
function DeletedUserRow({ menu }: { menu: React.ReactNode }) {
  return (
    <TableRow className="text-muted-foreground">
      <TableCell className="italic">{m.users_deleted_account()}</TableCell>
      <TableCell>—</TableCell>
      <TableCell>—</TableCell>
      <TableCell>—</TableCell>
      <TableCell className="text-right">—</TableCell>
      {menu}
    </TableRow>
  );
}
