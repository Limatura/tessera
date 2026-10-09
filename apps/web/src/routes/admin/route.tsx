import { Outlet, createFileRoute, useMatches } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";

import { AdminShell } from "@/components/admin/admin-shell";
import { requireAuth } from "@/lib/route-guard";
import * as m from "@/paraglide/messages";

/**
 * 後台的外框。側欄跟標題列放在這層，換頁時只有內容區重畫——放在各頁的話，
 * 每次換頁整個側欄都會 unmount 再 mount，展開的設定群組跟捲動位置都會掉。
 *
 * 標題列文字取最深一層有設 `staticData.title` 的路由(設定子頁沿用
 * `settings/route.tsx` 那個)。存翻譯函式不是字串：`staticData` 在模組載入時
 * 就定了，存字串切語言不會跟著變。
 *
 * 這層只擋「要登入」，因為側欄要讀 session；各頁要的權限不同，仍由各自的
 * `beforeLoad` 檢查。`reducedMotion="user"`：系統開「減少動態效果」時關掉後台的位移動畫。
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
    <MotionConfig reducedMotion="user">
      <AdminShell title={title?.() ?? m.admin_app_name()}>
        <Outlet />
      </AdminShell>
    </MotionConfig>
  );
}
