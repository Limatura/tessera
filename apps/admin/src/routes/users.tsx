import {
  IconDots,
  IconPlayerPause,
  IconPlayerPlay,
  IconShieldCog,
  IconTrash,
  IconTrashX,
} from "@tabler/icons-react";
import { createRoute, useNavigate } from "@tanstack/react-router";
import type { components } from "@tessera/api/schema";
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

import { AdminShell } from "@/components/admin-shell";
import { Pager, pageSearchSchema } from "@/components/pager";
import {
  PAGE_SIZE,
  useRoles,
  useSetUserState,
  useUsers,
} from "@/hooks/use-admin";
import { useSession } from "@/hooks/use-auth";
import { isDeletedAccount } from "@/lib/accounts";
import {
  accountRoleLabel,
  hasPermission,
  isUnassigned,
  PERMISSIONS,
} from "@/lib/permissions";
import { requirePermission } from "@/lib/route-guard";

import { Route as rootRoute } from "./__root";
import { SetUserRoleDialog } from "./users/-components/set-user-role-dialog";
import type { UserAction } from "./users/-components/user-action-dialog";
import { UserActionDialog } from "./users/-components/user-action-dialog";

type Account = components["schemas"]["AccountResponse"];

const usersSearchSchema = pageSearchSchema.extend({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  deleted: z.boolean().optional().catch(false),
});

export const Route = createRoute({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.usersRead
    ),
  component: UsersComponent,
  getParentRoute: () => rootRoute,
  path: "/users",
  validateSearch: usersSearchSchema,
});

/** 這列帳號上,目前登入的人能做哪些事。都不能做時整個選單不出現。 */
interface RowAbilities {
  setRole: boolean;
  setState: boolean;
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
        remove: false,
        removeShares: canRemoveShares,
        setRole: false,
        setState: false,
      };
    }
    // 停用或刪掉自己要走前台的帳號設定(刪自己要密碼),這裡不給。
    const isSelf = user.id === account?.id;
    return {
      remove: canDelete && !isSelf,
      removeShares: canRemoveShares,
      setRole: canSetRole,
      setState: canSetState && !isSelf,
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
          toast.add({ title: `無法啟用「${user.username}」`, type: "error" }),
        onSuccess: () =>
          toast.add({ title: `已啟用「${user.username}」`, type: "success" }),
      }
    );
  };

  return (
    <AdminShell title="使用者">
      <Card className="ring-foreground/10">
        <CardHeader>
          <CardTitle>帳號</CardTitle>
          <CardDescription>
            「未指派」代表還沒有人決定這個帳號能做什麼，跟刻意給訪客角色不同。
          </CardDescription>
          <CardAction className="flex items-center gap-2">
            <Switch
              checked={deleted}
              id="users-show-deleted"
              onCheckedChange={handleShowDeletedChange}
              size="sm"
            />
            <Label htmlFor="users-show-deleted">顯示已刪除的帳號</Label>
          </CardAction>
        </CardHeader>
        <CardContent>
          {users.isPending && (
            <p className="text-sm text-muted-foreground">載入中…</p>
          )}
          {users.isError && (
            <p className="text-sm text-destructive">帳號載入失敗</p>
          )}
          {users.data && (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>帳號</TableHead>
                  <TableHead>電子郵件</TableHead>
                  <TableHead>角色</TableHead>
                  <TableHead className="text-right">權限數</TableHead>
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
                            ? "已刪除的帳號"
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
                            尚未決定能做什麼
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
              to="/users"
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
      <TableCell className="italic">已刪除的帳號</TableCell>
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
  const { setRole, setState, removeShares, remove } = abilities;
  if (!(setRole || setState || removeShares || remove)) {
    return null;
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            aria-label={`${username} 的操作`}
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
            設定角色
          </DropdownMenuItem>
        )}
        {/*
         * 列表還看不到帳號是不是停用中(AdminUserResponse 還沒有 state),
         * 所以兩個都給。重複停用或啟用對 server 來說都是無害的。
         */}
        {setState && (
          <>
            <DropdownMenuItem onClick={() => onAction("disable")}>
              <IconPlayerPause />
              停用帳號
            </DropdownMenuItem>
            <DropdownMenuItem onClick={onEnable}>
              <IconPlayerPlay />
              重新啟用
            </DropdownMenuItem>
          </>
        )}
        {(removeShares || remove) && (setRole || setState) && (
          <DropdownMenuSeparator />
        )}
        {removeShares && (
          <DropdownMenuItem
            onClick={() => onAction("removeShares")}
            variant="destructive"
          >
            <IconTrashX />
            結束全部分享
          </DropdownMenuItem>
        )}
        {remove && (
          <DropdownMenuItem
            onClick={() => onAction("delete")}
            variant="destructive"
          >
            <IconTrash />
            刪除帳號
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
