import {
  IconArrowsSort,
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

import type { NavEntry, NavLink } from "@/components/admin/nav-main";
import { NavMain } from "@/components/admin/nav-main";
import { NavUser } from "@/components/admin/nav-user";
import { useSession } from "@/hooks/use-auth";
import type { Permission } from "@/lib/admin/permissions";
import { PERMISSIONS, hasPermission } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

/** 設定的各個分頁。整組共用 `settings:read`，所以只在外層擋一次。 */
function settingsItems(): NavLink[] {
  return [
    { title: m.nav_settings_account(), url: "/admin/settings/account" },
    {
      title: m.nav_settings_auth_providers(),
      url: "/admin/settings/auth-providers",
    },
    { title: m.nav_settings_password(), url: "/admin/settings/password" },
    { title: m.nav_settings_throttle(), url: "/admin/settings/throttle" },
    { title: m.nav_settings_mail(), url: "/admin/settings/mail" },
    { title: m.nav_settings_network(), url: "/admin/settings/network" },
    { title: m.nav_settings_share(), url: "/admin/settings/share" },
    { title: m.nav_settings_upload(), url: "/admin/settings/upload" },
    { title: m.nav_settings_storage(), url: "/admin/settings/storage" },
    { title: m.nav_settings_site(), url: "/admin/settings/site" },
    { title: m.nav_settings_theme(), url: "/admin/settings/theme" },
    { title: m.nav_settings_og(), url: "/admin/settings/og" },
    { title: m.nav_settings_features(), url: "/admin/settings/features" },
  ];
}

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
    {
      icon: <IconSettings />,
      items: settingsItems(),
      permission: PERMISSIONS.settingsRead,
      title: m.nav_settings(),
    },
  ];
}

function devItems() {
  return [
    {
      icon: <IconArrowsSort />,
      title: m.nav_dnd_demo(),
      url: "/admin/dnd-demo",
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
        {/* 開發工具只在 dev 顯示，正式 build 不出現（#370）。 */}
        {import.meta.env.DEV && (
          <NavMain items={devItems()} label={m.nav_dev_tools()} />
        )}
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  );
}
