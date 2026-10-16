import { IconBug, IconFileDescription } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@tessera/ui/components/sidebar";
import type * as React from "react";

import { useAdminNav } from "@/components/admin/nav-items";
import { NavMain } from "@/components/admin/nav-main";
import { NavUser } from "@/components/admin/nav-user";
import { Logo } from "@/components/logo";
import { getLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";

// The docs URL is a placeholder until the docs site has a public address.
function helpLinks(): { icon: React.ReactNode; title: string; url: string }[] {
  return [
    {
      icon: <IconBug />,
      title: m.nav_report_bug(),
      url: "https://github.com/Limatura/tessera/issues",
    },
    {
      icon: <IconFileDescription />,
      title: m.nav_docs(),
      url: `https://tessera-docs-three.vercel.app/${getLocale()}/docs`,
    },
  ];
}

/**
 * Collapses to an icon rail on desktop (Ctrl/⌘+B or the header button) and is
 * a sheet over the page on phones. Language moved into the account menu: a
 * select in the sidebar's footer took a whole row for something set once.
 */
export function AppSidebar({ ...props }: React.ComponentProps<typeof Sidebar>) {
  const groups = useAdminNav();

  return (
    <Sidebar collapsible="icon" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              className="data-[slot=sidebar-menu-button]:p-1.5!"
              render={
                <Link aria-label={m.app_sidebar_home_aria()} to="/admin" />
              }
              tooltip="Tessera Admin"
            >
              <Logo className="size-7! shrink-0" />
              <span className="font-semibold text-base">Tessera Admin</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain groups={groups} />
        <SidebarGroup className="mt-auto">
          <SidebarGroupContent>
            <SidebarMenu>
              {helpLinks().map((link) => (
                <SidebarMenuItem key={link.url}>
                  <SidebarMenuButton
                    render={
                      <a
                        aria-label={link.title}
                        href={link.url}
                        rel="noopener noreferrer"
                        target="_blank"
                      />
                    }
                    size="sm"
                    tooltip={link.title}
                  >
                    {link.icon}
                    <span>{link.title}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  );
}
