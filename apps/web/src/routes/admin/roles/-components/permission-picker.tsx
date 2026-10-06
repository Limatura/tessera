import type { components } from "@tessera/api/schema";
import { Checkbox } from "@tessera/ui/components/checkbox";

import * as m from "@/paraglide/messages";

type PermissionInfo = components["schemas"]["PermissionResponse"];

export function PermissionPicker({
  allPermissions,
  selected,
  grantable,
  frozen,
  onToggle,
}: {
  allPermissions: PermissionInfo[];
  selected: ReadonlySet<string>;
  /** 目前登入者自己持有的權限；沒有的授不出去。 */
  grantable: ReadonlySet<string>;
  /** 內建角色或送出中 —— 整組都不能動。 */
  frozen: boolean;
  onToggle: (permission: string, checked: boolean) => void;
}) {
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      {groupByResource(allPermissions).map(([resource, items]) => (
        <div className="flex flex-col gap-2" key={resource}>
          <p className="font-medium text-sm">{resource}</p>
          {items.map((permission) => {
            const checked = selected.has(permission.name);
            // 沒有的權限不能授出去，但已經在角色身上的仍要顯示成勾選，
            // 否則畫面會看起來像「這個角色沒有這個權限」。
            const locked =
              frozen || (!grantable.has(permission.name) && !checked);
            return (
              <label
                className="flex items-start gap-2 text-sm"
                key={permission.name}
              >
                <Checkbox
                  checked={checked}
                  className="mt-0.5"
                  disabled={locked}
                  onCheckedChange={(next) =>
                    onToggle(permission.name, next === true)
                  }
                />
                <span className="flex flex-col">
                  <span className="font-mono">{permission.name}</span>
                  {permission.privileged && (
                    <span className="text-destructive text-xs">
                      {m.roles_permission_picker_privileged_note()}
                    </span>
                  )}
                </span>
              </label>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** 依 `resource:verb` 的 resource 分組，讓一長串 checkbox 讀得下去。 */
function groupByResource(
  permissions: PermissionInfo[]
): [string, PermissionInfo[]][] {
  const groups = new Map<string, PermissionInfo[]>();
  for (const permission of permissions) {
    const [resource] = permission.name.split(":");
    const existing = groups.get(resource);
    if (existing) {
      existing.push(permission);
    } else {
      groups.set(resource, [permission]);
    }
  }
  return [...groups.entries()];
}
