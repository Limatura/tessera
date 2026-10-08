import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { Checkbox } from "@tessera/ui/components/checkbox";

import {
  HIGH_RISK_PERMISSIONS,
  permissionGroupLabel,
  permissionHint,
  permissionLabel,
} from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

type PermissionInfo = components["schemas"]["PermissionResponse"];

export function PermissionPicker({
  allPermissions,
  selected,
  grantable,
  disabled,
  onToggle,
}: {
  allPermissions: PermissionInfo[];
  selected: ReadonlySet<string>;
  /** 目前登入者自己持有的權限；沒有的授不出去。 */
  grantable: ReadonlySet<string>;
  /** 送出中 —— 整組暫時不能動。 */
  disabled: boolean;
  onToggle: (permission: string, checked: boolean) => void;
}) {
  return (
    <PermissionGroups permissions={allPermissions.map((p) => p.name)}>
      {(permission) => {
        const checked = selected.has(permission);
        // 沒有的權限不能授出去，但已經在角色身上的仍要顯示成勾選，
        // 否則畫面會看起來像「這個角色沒有這個權限」。
        const locked = disabled || (!grantable.has(permission) && !checked);
        const id = `permission-${permission}`;
        return (
          <label
            className="flex items-start gap-2 text-sm"
            htmlFor={id}
            key={permission}
          >
            <Checkbox
              checked={checked}
              className="mt-0.5"
              disabled={locked}
              id={id}
              onCheckedChange={(next) => onToggle(permission, next === true)}
            />
            <PermissionText permission={permission} />
          </label>
        );
      }}
    </PermissionGroups>
  );
}

/** 內建角色的權限不能改，一排停用的 checkbox 不好讀，直接列出它有的。 */
export function PermissionList({
  permissions,
}: {
  permissions: readonly string[];
}) {
  if (permissions.length === 0) {
    return (
      <p className="text-muted-foreground text-sm">
        {m.roles_editor_permissions_none()}
      </p>
    );
  }
  return (
    <PermissionGroups permissions={permissions}>
      {(permission) => (
        <div className="text-sm" key={permission}>
          <PermissionText permission={permission} />
        </div>
      )}
    </PermissionGroups>
  );
}

function PermissionGroups({
  permissions,
  children,
}: {
  permissions: readonly string[];
  children: (permission: string) => React.ReactNode;
}) {
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      {groupByResource(permissions).map(([resource, items]) => (
        <div className="flex flex-col gap-2" key={resource}>
          <p className="font-medium text-sm">
            {permissionGroupLabel(resource)}
          </p>
          {items.map(children)}
        </div>
      ))}
    </div>
  );
}

function PermissionText({ permission }: { permission: string }) {
  const hint = permissionHint(permission);
  return (
    <span className="flex flex-col gap-0.5">
      <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <span>{permissionLabel(permission)}</span>
        <code className="text-muted-foreground text-xs">{permission}</code>
        {HIGH_RISK_PERMISSIONS.has(permission) && (
          <Badge variant="destructive">{m.roles_permission_high_risk()}</Badge>
        )}
      </span>
      {hint && <span className="text-muted-foreground text-xs">{hint}</span>}
    </span>
  );
}

/** 依 `resource:verb` 的 resource 分組，讓一長串權限讀得下去。 */
function groupByResource(permissions: readonly string[]): [string, string[]][] {
  const groups = new Map<string, string[]>();
  for (const permission of permissions) {
    const [resource] = permission.split(":");
    const existing = groups.get(resource);
    if (existing) {
      existing.push(permission);
    } else {
      groups.set(resource, [permission]);
    }
  }
  return [...groups.entries()];
}
