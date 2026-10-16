import { useRouterState } from "@tanstack/react-router";
import { SidebarInset, SidebarProvider } from "@tessera/ui/components/sidebar";
import type * as React from "react";

import { AppSidebar } from "@/components/admin/app-sidebar";
import type { Crumb } from "@/components/admin/site-header";
import { SiteHeader } from "@/components/admin/site-header";
import { PageTransition } from "@/components/page-transition";

/**
 * `/admin/settings/mail` → `settings`: switching tabs inside settings is not a
 * new section, so the shell does not replay.
 */
function sectionOf(pathname: string) {
  return pathname.split("/")[2] ?? "";
}

/**
 * The admin frame: sidebar, title bar and content area. Rendered once by the
 * `/admin` layout, so on navigation the sidebar and title bar stay put and only
 * the content replays its entrance animation.
 */
export function AdminShell({
  crumbs,
  children,
}: {
  crumbs: Crumb[];
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
        <SiteHeader crumbs={crumbs} />
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
