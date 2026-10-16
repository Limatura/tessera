import { Link } from "@tanstack/react-router";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@tessera/ui/components/sidebar";

import type { AdminNavGroup } from "@/components/admin/nav-items";

/**
 * The sidebar's page links, one labelled group each. The settings tabs are
 * switched by the settings page's own grouped menu instead of being listed here.
 *
 * `Link` matches active routes by prefix, so "Settings" is also highlighted on
 * `/admin/settings/mail`. On a phone the sidebar is a sheet over the page, so a
 * tap closes it; otherwise it would stay over the page just opened.
 */
export function NavMain({ groups }: { groups: AdminNavGroup[] }) {
  const { isMobile, setOpenMobile } = useSidebar();

  return groups.map((group) => (
    <SidebarGroup key={group.title}>
      <SidebarGroupLabel>{group.title}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {group.items.map((item) => (
            <SidebarMenuItem key={item.url}>
              <SidebarMenuButton
                className="max-md:h-10"
                render={
                  <Link
                    activeProps={{ "data-active": true }}
                    onClick={() => isMobile && setOpenMobile(false)}
                    to={item.url}
                  />
                }
                tooltip={item.title}
              >
                {item.icon}
                <span>{item.title}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  ));
}
