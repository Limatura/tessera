import type { components } from "@tessera/api/schema";
import { useState } from "react";

import { modeOf, toBytes, toMib } from "@/components/admin/byte-limit-field";
import type { ByteLimitMode } from "@/components/admin/byte-limit-field";
import { useCreateRole, useUpdateRole } from "@/hooks/admin/use-admin";
import { hasDefaultRoleDescription } from "@/lib/admin/permissions";

type Role = components["schemas"]["RoleResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * 角色表單的狀態與送出。
 *
 * 初始值直接交給 `useState`，靠外層的 `key` 換角色時重新掛載，不用 effect 同步 ——
 * 那會在列表背景更新時把使用者打到一半的內容洗掉。
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
  // 內建角色還是後端寫的英文預設說明時，欄位留空、placeholder 顯示翻譯；
  // 存檔會變成 null，畫面就一直跟著語系顯示（#376）。
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

  // server 檢查的是送出去的「整組」權限，不是差異。所以角色身上只要有一個自己
  // 沒有的權限，這次編輯就會被整個拒絕 —— 與其讓人填完才吃 403，先講清楚。
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
