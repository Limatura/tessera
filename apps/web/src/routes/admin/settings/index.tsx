import { createFileRoute, redirect } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";

/**
 * `/settings` 本身沒有畫面 —— 設定拆成側邊欄裡的分頁後，這裡只負責把舊連結、舊書籤
 * 送到第一個分頁去。
 *
 * 權限先驗再轉：沒有 `settings:read` 的人應該被送去他自己進得去的頁面，而不是先跳到
 * 一個同樣進不去的子頁再被轉一次。
 */
export const Route = createFileRoute("/admin/settings/")({
  beforeLoad: async ({ context, location }) => {
    await requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    );
    throw redirect({ to: "/admin/settings/account" });
  },
});
