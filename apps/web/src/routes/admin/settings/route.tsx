import { createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

/**
 * 設定的每個分頁都只要 `settings:read`,所以在這層擋一次;標題列也都是「系統設定」,
 * 子頁只管自己那幾張卡。
 *
 * 沒有 `component`,預設就是 `<Outlet />`。
 */
export const Route = createFileRoute("/admin/settings")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  staticData: { title: m.settings_page_shell_title },
});
