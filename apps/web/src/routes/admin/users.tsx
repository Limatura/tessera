import {
  IconDots,
  IconPlayerPause,
  IconPlayerPlay,
  IconShieldCog,
  IconTrash,
  IconTrashX,
} from "@tabler/icons-react";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@tessera/ui/components/dropdown-menu";
import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@tessera/ui/components/table";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";
import { z } from "zod";

import { AdminShell } from "@/components/admin/admin-shell";
import { Pager, pageSearchSchema } from "@/components/admin/pager";
import {
  PAGE_SIZE,
  useRoles,
  useSetUserState,
  useUsers,
} from "@/hooks/admin/use-admin";
import { useSession } from "@/hooks/use-auth";
import type { AdminUser } from "@/lib/admin/accounts";
import { isDeletedAccount } from "@/lib/admin/accounts";
import {
  accountRoleLabel,
  hasPermission,
  isUnassigned,
  PERMISSIONS,
} from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { SetUserRoleDialog } from "./users/-components/set-user-role-dialog";
import type { UserAction } from "./users/-components/user-action-dialog";
import { UserActionDialog } from "./users/-components/user-action-dialog";

type Account = AdminUser;

const usersSearchSchema = pageSearchSchema.extend({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  deleted: z.boolean().optional().catch(false),
});

export const Route = createFileRoute("/admin/users")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.usersRead
    ),
  component: UsersComponent,
  validateSearch: usersSearchSchema,
});

/** 這列帳號上,目前登入的人能做哪些事。都不能做時整個選單不出現。 */
interface RowAbilities {
  setRole: boolean;
  disable: boolean;
  enable: boolean;
  removeShares: boolean;
  remove: boolean;
}

function UsersComponent() {
  const { page, deleted = false } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const users = useUsers((page - 1) * PAGE_SIZE, deleted);
  const roles = useRoles();
  const { data: account } = useSession();
  const setUserState = useSetUserState();

  const [editing, setEditing] = useState<Account | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [pending, setPending] = useState<{
    account: Account;
    action: UserAction;
  } | null>(null);

  // #123 之後 admin 沒有 roles:write / users:delete / shares:delete:any,
  // 每個動作各看各的權限,不要拿 users:write 一次概括。
  const canSetRole = hasPermission(account, PERMISSIONS.rolesWrite);
  const canSetState = hasPermission(account, PERMISSIONS.usersWrite);
  const canRemoveShares = hasPermission(account, PERMISSIONS.sharesDeleteAny);
  const canDelete = hasPermission(account, PERMISSIONS.usersDelete);
  const hasActions = canSetRole || canSetState || canRemoveShares || canDelete;

  const abilitiesFor = (user: Account): RowAbilities => {
    // 殘骸不代表任何人,能做的只剩清掉它留下的分享 —— 刪帳號本來就不會帶走那些。
    if (isDeletedAccount(user)) {
      return {
        disable: false,
        enable: false,
        remove: false,
        removeShares: canRemoveShares,
        setRole: false,
      };
    }
    // 停用或刪掉自己要走前台的帳號設定(刪自己要密碼),這裡不給。
    // 改自己的角色也不給:後端只擋「改完沒人能管理」,還有別的管理員時
    // 首次啟動的 owner 會被自己降級(#351)。
    const isSelf = user.id === account?.id;
    return {
      disable: canSetState && !isSelf && user.state === "active",
      enable: canSetState && user.state === "disabled",
      remove: canDelete && !isSelf,
      removeShares: canRemoveShares,
      setRole: canSetRole && !isSelf,
    };
  };

  const handleShowDeletedChange = (checked: boolean) => {
    navigate({ search: { deleted: checked || undefined, page: 1 } });
  };

  const handleEditRole = (user: Account) => {
    setEditing(user);
    setEditorOpen(true);
  };

  const handleEnable = (user: Account) => {
    setUserState.mutate(
      { id: user.id, state: "active" },
      {
        onError: () =>
          toast.add({
            title: m.users_enable_error({ name: user.username }),
            type: "error",
          }),
        onSuccess: () =>
          toast.add({
            title: m.users_enable_success({ name: user.username }),
            type: "success",
          }),
      }
    );
  };

  return (
    <AdminShell title={m.users_page_title()}>
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>{m.users_card_title()}</CardTitle>
          <CardDescription>{m.users_card_description()}</CardDescription>
          <CardAction className="flex items-center gap-2">
            <Switch
              checked={deleted}
              id="users-show-deleted"
              onCheckedChange={handleShowDeletedChange}
              size="sm"
            />
            <Label htmlFor="users-show-deleted">{m.users_show_deleted()}</Label>
          </CardAction>
        </CardHeader>
        <CardContent>
          {users.isPending && (
            <p className="text-sm text-muted-foreground">
              {m.common_loading()}
            </p>
          )}
          {users.isError && (
            <p className="text-sm text-destructive">{m.users_load_error()}</p>
          )}
          {users.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{m.users_col_account()}</TableHead>
                  <TableHead>{m.users_col_email()}</TableHead>
                  <TableHead>{m.users_col_role()}</TableHead>
                  <TableHead className="text-right">
                    {m.users_col_permission_count()}
                  </TableHead>
                  {hasActions && <TableHead className="w-12" />}
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.data.data.map((user) => {
                  const menu = hasActions && (
                    <TableCell>
                      <UserRowMenu
                        abilities={abilitiesFor(user)}
                        onAction={(action) =>
                          setPending({ account: user, action })
                        }
                        onEditRole={() => handleEditRole(user)}
                        onEnable={() => handleEnable(user)}
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
                        <Badge
                          variant={isUnassigned(user) ? "outline" : "secondary"}
                        >
                          {accountRoleLabel(user)}
                        </Badge>
                        {isUnassigned(user) && (
                          <span className="ml-2 text-xs text-muted-foreground">
                            {m.users_not_decided()}
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
          )}
          {users.data && (
            <Pager
              page={page}
              pageSize={PAGE_SIZE}
              to="/admin/users"
              total={users.data.meta.total}
            />
          )}
        </CardContent>
      </Card>

      {canSetRole && (
        <SetUserRoleDialog
          account={editing}
          onOpenChange={setEditorOpen}
          open={editorOpen}
          roles={roles.data ?? []}
        />
      )}
      <UserActionDialog
        account={pending?.account ?? null}
        action={pending?.action ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setPending(null);
          }
        }}
      />
    </AdminShell>
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

function UserRowMenu({
  username,
  abilities,
  onEditRole,
  onEnable,
  onAction,
}: {
  username: string;
  abilities: RowAbilities;
  onEditRole: () => void;
  onEnable: () => void;
  onAction: (action: UserAction) => void;
}) {
  const { setRole, disable, enable, removeShares, remove } = abilities;
  if (!(setRole || disable || enable || removeShares || remove)) {
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
          <DropdownMenuItem onClick={onEditRole}>
            <IconShieldCog />
            {m.users_menu_set_role()}
          </DropdownMenuItem>
        )}
        {disable && (
          <DropdownMenuItem onClick={() => onAction("disable")}>
            <IconPlayerPause />
            {m.users_menu_disable()}
          </DropdownMenuItem>
        )}
        {enable && (
          <DropdownMenuItem onClick={onEnable}>
            <IconPlayerPlay />
            {m.users_menu_enable()}
          </DropdownMenuItem>
        )}
        {(removeShares || remove) && (setRole || disable || enable) && (
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
