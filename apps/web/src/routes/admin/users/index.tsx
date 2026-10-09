import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@tessera/ui/components/card";
import { Label } from "@tessera/ui/components/label";
import { Switch } from "@tessera/ui/components/switch";
import { toast } from "@tessera/ui/components/toast";
import { useState } from "react";
import { z } from "zod";

import { Pager } from "@/components/admin/pager";
import { useSetUserState, useUsers } from "@/hooks/admin/use-users";
import type { AdminUser } from "@/lib/admin/accounts";
import { isDeletedAccount } from "@/lib/admin/accounts";
import { PAGE_SIZE, pageSearchSchema } from "@/lib/admin/page-search";
import { hasPermission, PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { SetUserRoleDialog } from "./-components/set-user-role-dialog";
import type { UserAction } from "./-components/user-action-dialog";
import { UserActionDialog } from "./-components/user-action-dialog";
import { UserQuotaDialog } from "./-components/user-quota-dialog";
import type { RowAbilities, RowAction } from "./-components/users-table";
import { UsersTable } from "./-components/users-table";

type Account = AdminUser;

const usersSearchSchema = pageSearchSchema.extend({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod 的 catch,不是 Promise 的
  deleted: z.boolean().optional().catch(false),
});

export const Route = createFileRoute("/admin/users/")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.usersRead
    ),
  component: UsersComponent,
  staticData: { title: m.users_page_title },
  validateSearch: usersSearchSchema,
});

function UsersComponent() {
  const { page, deleted = false } = Route.useSearch();
  const navigate = useNavigate({ from: Route.fullPath });
  const users = useUsers((page - 1) * PAGE_SIZE, deleted);
  const { data: account } = useSession();
  const setUserState = useSetUserState();

  const [editing, setEditing] = useState<Account | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [quotaFor, setQuotaFor] = useState<Account | null>(null);
  const [quotaOpen, setQuotaOpen] = useState(false);
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
        setQuota: false,
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
      // 配額改的是 PATCH /admin/users/{id}，跟停用同一個權限。自己的也可以改。
      setQuota: canSetState,
      setRole: canSetRole && !isSelf,
    };
  };

  const handleShowDeletedChange = (checked: boolean) => {
    navigate({ search: { deleted: checked || undefined, page: 1 } });
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

  const handleRowAction = (user: Account, action: RowAction) => {
    switch (action) {
      case "setRole": {
        setEditing(user);
        setEditorOpen(true);
        break;
      }
      case "setQuota": {
        setQuotaFor(user);
        setQuotaOpen(true);
        break;
      }
      case "enable": {
        handleEnable(user);
        break;
      }
      default: {
        setPending({ account: user, action });
      }
    }
  };

  return (
    <>
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
            <UsersTable
              abilitiesFor={hasActions ? abilitiesFor : null}
              onAction={handleRowAction}
              users={users.data.data}
            />
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
        />
      )}
      {canSetState && (
        <UserQuotaDialog
          account={quotaFor}
          onOpenChange={setQuotaOpen}
          open={quotaOpen}
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
    </>
  );
}
