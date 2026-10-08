import { createFileRoute } from "@tanstack/react-router";

import { StorageBackendsCard } from "./-components/storage-backends-card";
import { StorageSettingsForm } from "./-components/storage-settings-form";

export const Route = createFileRoute("/admin/settings/storage")({
  component: StorageSettingsRoute,
});

/**
 * 兩張各自獨立的卡：分片上傳大小走 `PUT /admin/settings`，backend 列表走
 * `/admin/storage/backends` 系列端點——兩邊互不相干，合成一份表單只會讓其中一邊
 * 的錯誤擋住另一邊。
 */
function StorageSettingsRoute() {
  return (
    <>
      <StorageSettingsForm />
      <StorageBackendsCard />
    </>
  );
}
