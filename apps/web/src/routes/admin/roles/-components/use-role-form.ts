import type { components } from "@tessera/api/schema";
import { useState } from "react";

import { modeOf, toBytes, toMib } from "@/components/admin/byte-limit-field";
import type { ByteLimitMode } from "@/components/admin/byte-limit-field";
import { useCreateRole, useUpdateRole } from "@/hooks/admin/use-roles";
import { hasDefaultRoleDescription } from "@/lib/admin/permissions";

type Role = components["schemas"]["RoleResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * State and submission of the role form.
 *
 * Initial values go straight into `useState`, and the outer `key` remounts the form
 * when the role changes. Syncing with an effect would wipe half-typed input whenever
 * the list refreshes in the background.
 */
export function useRoleForm({
  role,
  grantable,
  onDone,
}: {
  role: Role | null;
  grantable: ReadonlySet<string>;
  onDone: () => void;
}) {
  const createRole = useCreateRole();
  const updateRole = useUpdateRole();

  const [name, setName] = useState(role?.name ?? "");
  // While a built-in role still has the backend's English default description, the
  // field stays empty and the placeholder shows the translation; saving stores null, so
  // the description keeps following the locale.
  const [description, setDescription] = useState(
    role && hasDefaultRoleDescription(role) ? "" : (role?.description ?? "")
  );
  const [selected, setSelected] = useState<ReadonlySet<string>>(
    new Set(role?.permissions)
  );
  const [quotaMode, setQuotaMode] = useState<ByteLimitMode>(
    modeOf(role?.quota_bytes)
  );
  const [quotaMib, setQuotaMib] = useState(toMib(role?.quota_bytes));
  const [fileMode, setFileMode] = useState<ByteLimitMode>(
    modeOf(role?.max_file_bytes)
  );
  const [fileMib, setFileMib] = useState(toMib(role?.max_file_bytes));

  const isSystem = role?.is_system ?? false;
  const pending = createRole.isPending || updateRole.isPending;
  const error = (createRole.error ?? updateRole.error) as ErrorBody | null;

  // The server checks the full set of permissions sent, not the difference, so one
  // permission on the role that you lack rejects the edit. Say so up front instead of
  // a 403 after everything is filled in.
  const ungrantable = [...selected].filter(
    (permission) => !grantable.has(permission)
  );

  const toggle = (permission: string, checked: boolean) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (checked) {
        next.add(permission);
      } else {
        next.delete(permission);
      }
      return next;
    });
  };

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    const body = {
      description: description.trim() || null,
      max_file_bytes: toBytes(fileMode, fileMib),
      name: name.trim(),
      permissions: [...selected],
      quota_bytes: toBytes(quotaMode, quotaMib),
    };

    if (role) {
      updateRole.mutate({ body, id: role.id }, { onSuccess: onDone });
    } else {
      createRole.mutate(body, { onSuccess: onDone });
    }
  };

  return {
    blocked: ungrantable.length > 0,
    description,
    error,
    fileMib,
    fileMode,
    isSystem,
    name,
    pending,
    quotaMib,
    quotaMode,
    selected,
    setDescription,
    setFileMib,
    setFileMode,
    setName,
    setQuotaMib,
    setQuotaMode,
    submit,
    toggle,
    ungrantable,
  };
}
