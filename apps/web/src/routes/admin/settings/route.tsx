import { Outlet, createFileRoute } from "@tanstack/react-router";

import { PERMISSIONS } from "@/lib/admin/permissions";
import { requirePermission } from "@/lib/admin/route-guard";
import * as m from "@/paraglide/messages";

import { SettingsNav } from "./-components/settings-nav";

/**
 * 所有設定分頁共用的版面：左邊分組選單、右邊目前這一頁。側欄外框在上一層
 * `admin/route.tsx`，這層只管設定自己的選單。
 *
 * 權限也在這層擋一次 —— 整組設定都只要 `settings:read`;標題列也都是「系統設定」。
 */
export const Route = createFileRoute("/admin/settings")({
  beforeLoad: ({ context, location }) =>
    requirePermission(
      context.queryClient,
      location.href,
      PERMISSIONS.settingsRead
    ),
  component: SettingsLayout,
  staticData: { title: m.settings_page_shell_title },
});

/**
 * 選單加內容整組限寬並置中 —— 只限內容、整組靠左的話，寬螢幕右邊會空出一大片。
 * 每一頁用同一個寬度，切換分頁時左邊選單才不會跟著跳。
 */
function SettingsLayout() {
  return (
    <div className="mx-auto grid w-full max-w-6xl gap-6 lg:grid-cols-[12rem_minmax(0,1fr)] lg:gap-10">
      <SettingsNav />
      <div className="min-w-0">
        <Outlet />
      </div>
    </div>
  );
}
