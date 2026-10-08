import { Outlet, createFileRoute, useMatches } from "@tanstack/react-router";

import { AdminShell } from "@/components/admin/admin-shell";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

/**
 * 後台的外框。側欄跟標題列放在這層，換頁時只有內容區重畫 —— 放在各頁的話，每次換頁
 * 整個側欄都會 unmount 再 mount,展開的設定群組跟側欄的捲動位置都會掉。
 *
 * 標題列的文字取最深一層有設 `staticData.title` 的路由(設定的子頁沿用
 * `settings/route.tsx` 那個)。存的是翻譯函式不是字串：`staticData` 在模組載入時
 * 就定了，存字串的話切語言不會跟著變。
 *
 * 這層只擋「要登入」,因為側欄要讀 session。每頁要的權限不同，仍由各頁的
 * `beforeLoad` 各自檢查。
 */
export const Route = createFileRoute("/admin")({
  beforeLoad: ({ context, location }) =>
    requireAuth(context.queryClient, location.href),
  component: AdminLayout,
  head: () => ({
    meta: [
      {
        title: m.admin_app_name(),
      },
      {
        content: m.admin_app_description(),
        name: "description",
      },
    ],
  }),
});

function AdminLayout() {
  const title = useMatches({
    select: (matches) =>
      matches.findLast((match) => match.staticData.title)?.staticData.title,
  });

  return (
    <AdminShell title={title?.() ?? m.admin_app_name()}>
      <Outlet />
    </AdminShell>
  );
}
