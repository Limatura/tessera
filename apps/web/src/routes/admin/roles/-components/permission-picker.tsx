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
  /** Permissions the signed-in user holds; others cannot be granted. */
  grantable: ReadonlySet<string>;
  /** Submitting: every checkbox is locked for now. */
  disabled: boolean;
  onToggle: (permission: string, checked: boolean) => void;
}) {
  return (
    <PermissionGroups permissions={allPermissions.map((p) => p.name)}>
      {(permission) => {
        const checked = selected.has(permission);
        // A permission you lack cannot be granted, but one the role already has must
        // still show as checked, or the role would look like it lacks it.
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

/**
 * Built-in roles' permissions cannot change, and a row of disabled checkboxes
 * reads badly, so just list them.
 */
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
        {HIGH_RISK_PERMISSIONS.has(permission) && (
          <Badge variant="destructive">{m.roles_permission_high_risk()}</Badge>
        )}
      </span>
      {hint && <span className="text-muted-foreground text-xs">{hint}</span>}
    </span>
  );
}

/** Grouped by the resource in `resource:verb`, so a long list stays readable. */
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
