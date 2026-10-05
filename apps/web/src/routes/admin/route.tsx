import { Outlet, createFileRoute } from "@tanstack/react-router";

import * as m from "@/paraglide/messages";

/**
 * 後台的進入點。側欄外框由各頁的 `AdminShell` 自己帶標題，這裡只換掉分頁標題。
 *
 * 權限由各頁的 `beforeLoad` 各自檢查（每頁要的權限不同），不在這層統一擋。
 */
export const Route = createFileRoute("/admin")({
  component: Outlet,
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
