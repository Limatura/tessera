import {
  IconDashboard,
  IconDatabase,
  IconFiles,
  IconInnerShadowTop,
  IconKey,
  IconSettings,
  IconShieldLock,
  IconUsers,
} from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@tessera/ui/components/sidebar";
import type * as React from "react";

import type { NavEntry } from "@/components/admin/nav-main";
import { NavMain } from "@/components/admin/nav-main";
import { NavUser } from "@/components/admin/nav-user";
import type { Permission } from "@/lib/admin/permissions";
import { PERMISSIONS, hasPermission } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

/**
 * `permission` 為 undefined 代表只要登入就看得到。
 *
 * 這裡藏起來的項目只是不讓人點進一個會被轉走的頁面，真正的授權是每個路由的
 * `beforeLoad` 加上 server 端的檢查。
 */
function navItems(): (NavEntry & { permission?: Permission })[] {
  return [
    {
      icon: <IconDashboard />,
      permission: PERMISSIONS.statsRead,
      title: m.nav_dashboard(),
      url: "/admin/dashboard",
    },
    {
      icon: <IconFiles />,
      title: m.nav_my_shares(),
      url: "/admin/shares",
    },
    {
      icon: <IconUsers />,
      permission: PERMISSIONS.usersRead,
      title: m.nav_users(),
      url: "/admin/users",
    },
    {
      icon: <IconDatabase />,
      permission: PERMISSIONS.sharesReadAny,
      title: m.nav_all_shares(),
      url: "/admin/all-shares",
    },
    {
      icon: <IconKey />,
      permission: PERMISSIONS.invitesRead,
      title: m.nav_invites(),
      url: "/admin/invites",
    },
    {
      icon: <IconShieldLock />,
      permission: PERMISSIONS.rolesRead,
      title: m.nav_roles(),
      url: "/admin/roles",
    },
    // 設定的十幾個分頁不攤在這裡，進去之後由頁面左側的分組選單切換。
    {
      icon: <IconSettings />,
      permission: PERMISSIONS.settingsRead,
      title: m.nav_settings(),
      url: "/admin/settings",
    },
  ];
}

export function AppSidebar({ ...props }: React.ComponentProps<typeof Sidebar>) {
  const { data: account } = useSession();

  const items: NavEntry[] = navItems().filter(
    (item) => !item.permission || hasPermission(account, item.permission)
  );

  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              className="data-[slot=sidebar-menu-button]:p-1.5!"
              render={
                <Link aria-label={m.app_sidebar_home_aria()} to="/admin" />
              }
            >
              <IconInnerShadowTop className="size-5!" />
              <span className="text-base font-semibold">Tessera Admin</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={items} />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  );
}
