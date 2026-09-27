import type { components } from "@tessera/api/schema";
import { useState } from "react";
import type * as React from "react";

import { useAddStorageBackend } from "@/hooks/use-admin";

type Backend = components["schemas"]["BackendResponse"];
type ErrorBody = components["schemas"]["ErrorBody"];

const GIB = 1024 * 1024 * 1024;

/**
 * 新增或覆蓋一列 backend 的表單狀態。
 *
 * `url` 是 sealed：`GET` 從來不回傳它，所以編輯既有 backend 時這裡永遠是空白起頭 ——
 * 跟 `SealedField` 同一個道理，但這裡沒有「留空 = 不改」，`POST` 一定會覆蓋整列，
 * 所以編輯時把 `url` 標成必填，逼使用者重新輸入而不是誤以為留空能保留原本的值。
 */
export function useStorageBackendForm({
  backend,
  onDone,
}: {
  backend: Backend | null;
  onDone: () => void;
}) {
  const addBackend = useAddStorageBackend();
  const error = addBackend.error as ErrorBody | null;

  const [name, setName] = useState(backend?.name ?? "");
  const [url, setUrl] = useState("");
  const [capacityGib, setCapacityGib] = useState(
    backend?.capacity_bytes ? backend.capacity_bytes / GIB : 0
  );
  const [unlimited, setUnlimited] = useState(isUnlimited(backend));
  const [directAccess, setDirectAccess] = useState(
    backend?.direct_access ?? false
  );
  const [readOnly, setReadOnly] = useState(backend?.read_only ?? false);

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    addBackend.mutate(
      {
        capacity_bytes: unlimited ? null : Math.round(capacityGib * GIB),
        direct_access: directAccess,
        name: name.trim(),
        read_only: readOnly,
        url: url.trim(),
      },
      { onSuccess: onDone }
    );
  };

  return {
    capacityGib,
    canSubmit: name.trim() !== "" && url.trim() !== "",
    directAccess,
    error,
    name,
    pending: addBackend.isPending,
    readOnly,
    setCapacityGib,
    setDirectAccess,
    setName,
    setReadOnly,
    setUnlimited,
    setUrl,
    submit,
    unlimited,
    url,
  };
}

function isUnlimited(backend: Backend | null): boolean {
  if (!backend) {
    return true;
  }
  return (
    backend.capacity_bytes === null || backend.capacity_bytes === undefined
  );
}
