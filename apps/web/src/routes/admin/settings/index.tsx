import { createFileRoute, redirect } from "@tanstack/react-router";

/**
 * `/settings` 本身沒有畫面，只負責把舊連結、舊書籤跟側欄的「系統設定」送到分組選單
 * 的第一頁。權限在上一層 `route.tsx` 已經驗過了。
 */
export const Route = createFileRoute("/admin/settings/")({
  beforeLoad: () => {
    throw redirect({ to: "/admin/settings/site" });
  },
});
