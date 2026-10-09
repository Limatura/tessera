import {
  Outlet,
  createFileRoute,
  useRouterState,
} from "@tanstack/react-router";

import { Header } from "@/components/header";
import { PageTransition } from "@/components/page-transition";

/**
 * 前台頁面共用的外框：頂部 Header + 換頁淡入。
 *
 * pathless（`_site`）所以不佔網址。後台在 `/admin` 有自己的側欄外框，不吃這個 Header，
 * 所以 Header 不能放在 root。
 *
 * 動畫 key 用 pathname 不含 search：首頁切上傳分頁之類只改 query 的操作不該重播。
 * 包的那層是 `__root` grid 的第二列，要 `min-w-0`，理由同 `__root` 的 grid-cols-1。
 */
export const Route = createFileRoute("/_site")({
  component: SiteLayout,
});

function SiteLayout() {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });

  return (
    <>
      <Header />
      <PageTransition className="min-w-0" transitionKey={pathname}>
        <Outlet />
      </PageTransition>
    </>
  );
}
