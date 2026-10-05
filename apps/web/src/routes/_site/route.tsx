import { Outlet, createFileRoute } from "@tanstack/react-router";

import { Header } from "@/components/header";

/**
 * 前台頁面共用的外框：頂部 Header。
 *
 * pathless（`_site`）所以不佔網址。後台在 `/admin` 有自己的側欄外框，不吃這個 Header，
 * 所以 Header 不能放在 root。
 */
export const Route = createFileRoute("/_site")({
  component: SiteLayout,
});

function SiteLayout() {
  return (
    <>
      <Header />
      <Outlet />
    </>
  );
}
