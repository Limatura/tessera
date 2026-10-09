import { useRouterState } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import type * as React from "react";

import { AppSidebar } from "@/components/admin/app-sidebar";
import { SiteHeader } from "@/components/admin/site-header";
import { PageTransition } from "@/components/page-transition";

/** `/admin/settings/mail` → `settings`：設定內切分頁不算換區塊，外框不重播。 */
function sectionOf(pathname: string) {
  return pathname.split("/")[2] ?? "";
}

/**
 * 後台共用的外框：側邊欄 + 標題列 + 內容區。由 `/admin` layout 渲染一次，換頁時
 * 側欄跟標題列留著不動，只有內容區重播進場動畫。
 */
export function AdminShell({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  const section = useRouterState({
    select: (state) => sectionOf(state.location.pathname),
  });

  return (
    <SidebarProvider
      style={
        {
          "--header-height": "calc(var(--spacing) * 12)",
          "--sidebar-width": "calc(var(--spacing) * 72)",
        } as React.CSSProperties
      }
    >
      <AppSidebar variant="inset" />
      <SidebarInset>
        <SiteHeader title={title} />
        <div className="flex flex-1 flex-col">
          <PageTransition
            className="@container/main flex flex-1 flex-col gap-6 px-4 py-4 md:py-6 lg:px-6"
            transitionKey={section}
          >
            {children}
          </PageTransition>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}
