import {
  IconDots,
  IconGauge,
  IconPlayerPause,
  IconPlayerPlay,
  IconShieldCog,
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { formatBytes } from "@tessera/ui/lib/format";

import type { AdminUser } from "@/lib/admin/accounts";
import { isDeletedAccount } from "@/lib/admin/accounts";
import { accountRoleLabel, isUnassigned } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

import type { UserAction } from "./user-action-dialog";

/** 這列帳號上,目前登入的人能做哪些事。都不能做時整個選單不出現。 */
export interface RowAbilities {
  setRole: boolean;
  setQuota: boolean;
  disable: boolean;
  enable: boolean;
  removeShares: boolean;
  remove: boolean;
}

/** 選單上的動作。`UserAction` 那幾個要先確認，其餘直接開對話框或送出。 */
export type RowAction = "enable" | "setQuota" | "setRole" | UserAction;

export function UsersTable({
  users,
  abilitiesFor,
  onAction,
}: {
  users: AdminUser[];
  /** 目前登入的人什麼都不能做時給 `null`,動作欄整欄不出現。 */
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
                <QuotaBadge quotaBytes={user.quota_bytes} />
                <FileLimitBadge maxFileBytes={user.max_file_bytes} />
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

/** 殘骸那串 `gone-01K…` 對人沒有意義,只說它是什麼。 */
function DeletedUserRow({ menu }: { menu: React.ReactNode }) {
  return (
    <TableRow className="text-muted-foreground">
      <TableCell className="italic">{m.users_deleted_account()}</TableCell>
      <TableCell>—</TableCell>
      <TableCell>—</TableCell>
      <TableCell className="text-right">—</TableCell>
      {menu}
    </TableRow>
  );
}

/**
 * 帳號自己設了配額才顯示 —— 沒設（`null`）就是沿用角色，角色那欄已經說了。
 * `0` 是「這個帳號不限制」，跟沒設是兩回事，要看得出來。
 */
function QuotaBadge({ quotaBytes }: { quotaBytes?: number | null }) {
  if (quotaBytes === null || quotaBytes === undefined) {
    return null;
  }
  return (
    <Badge className="ml-2" variant="outline">
      {quotaBytes === 0
        ? m.users_quota_badge_unlimited()
        : m.users_quota_badge({ amount: formatBytes(quotaBytes) })}
    </Badge>
  );
}

function FileLimitBadge({ maxFileBytes }: { maxFileBytes?: number | null }) {
  if (maxFileBytes === null || maxFileBytes === undefined) {
    return null;
  }
  return (
    <Badge className="ml-2" variant="outline">
      {maxFileBytes === 0
        ? m.users_file_badge_unlimited()
        : m.users_file_badge({ amount: formatBytes(maxFileBytes) })}
    </Badge>
  );
}

function UserRowMenu({
  username,
  abilities,
  onAction,
}: {
  username: string;
  abilities: RowAbilities;
  onAction: (action: RowAction) => void;
}) {
  const { setRole, setQuota, disable, enable, removeShares, remove } =
    abilities;
  if (!(setRole || setQuota || disable || enable || removeShares || remove)) {
    return null;
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={m.users_row_menu_aria({ username })}
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
        {(removeShares || remove) &&
          (setRole || setQuota || disable || enable) && (
            <DropdownMenuSeparator />
          )}
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
