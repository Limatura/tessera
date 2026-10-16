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

import {
  ListEmpty,
  ListError,
  ListSkeleton,
} from "@/components/admin/list-state";
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
import type { RowAbilities, RowAction } from "./-components/user-row-parts";
import { UsersList } from "./-components/users-list";
import { UsersTable } from "./-components/users-table";

type Account = AdminUser;

const usersSearchSchema = pageSearchSchema.extend({
  // oxlint-disable-next-line promise/prefer-await-to-then -- zod's catch, not a Promise's
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

  // admin has no roles:write, users:delete or shares:delete:any, so each
  // action checks its own permission instead of using users:write for all.
  const canSetRole = hasPermission(account, PERMISSIONS.rolesWrite);
  const canSetState = hasPermission(account, PERMISSIONS.usersWrite);
  const canRemoveShares = hasPermission(account, PERMISSIONS.sharesDeleteAny);
  const canDelete = hasPermission(account, PERMISSIONS.usersDelete);
  const hasActions = canSetRole || canSetState || canRemoveShares || canDelete;

  const abilitiesFor = (user: Account): RowAbilities => {
    // A tombstone stands for no one. All that is left is removing the shares it
    // left behind, which deleting the account never did.
    if (isDeletedAccount(user)) {
      return {
        disable: false,
        enable: false,
        remove: false,
        removeShares: canRemoveShares,
        resetMfa: false,
        setQuota: false,
        setRole: false,
      };
    }
    // Disabling or deleting yourself goes through your own account settings
    // (deleting needs the password), so not here. Neither is changing your own
    // role: the server only blocks a change that leaves no one able to manage,
    // so with another admin around, the first-run owner could demote themselves.
    const isSelf = user.id === account?.id;
    return {
      disable: canSetState && !isSelf && user.state === "active",
      enable: canSetState && user.state === "disabled",
      remove: canDelete && !isSelf,
      removeShares: canRemoveShares,
      // Same permission as disabling. Not your own: that goes through your
      // account settings, which ask for a code first.
      resetMfa: canSetState && !isSelf && user.mfa_enabled,
      // Quotas go through PATCH /admin/users/{id}, the same permission as
      // disabling. Your own is allowed.
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
          {users.isPending && <ListSkeleton />}
          {users.isError && (
            <ListError
              isRetrying={users.isFetching}
              message={m.users_load_error()}
              onRetry={() => users.refetch()}
            />
          )}
          {users.data?.data.length === 0 && (
            <ListEmpty>{m.users_empty()}</ListEmpty>
          )}
          {users.data && users.data.data.length > 0 && (
            <>
              <div className="md:hidden">
                <UsersList
                  abilitiesFor={hasActions ? abilitiesFor : null}
                  onAction={handleRowAction}
                  users={users.data.data}
                />
              </div>
              <div className="hidden md:block">
                <UsersTable
                  abilitiesFor={hasActions ? abilitiesFor : null}
                  onAction={handleRowAction}
                  users={users.data.data}
                />
              </div>
            </>
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
