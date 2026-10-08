import { createFileRoute, redirect } from "@tanstack/react-router";

/**
 * `/settings` 本身沒有畫面 —— 設定拆成側邊欄裡的分頁後，這裡只負責把舊連結、舊書籤
 * 送到第一個分頁去。
 *
 * 權限由 `route.tsx` 先驗：沒有 `settings:read` 的人在那裡就被送去他自己進得去的頁面，
 * 不會先跳到一個同樣進不去的子頁再被轉一次。
 */
export const Route = createFileRoute("/admin/settings/")({
  beforeLoad: () => {
    throw redirect({ to: "/admin/settings/account" });
  },
});
