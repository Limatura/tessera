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

import type { NavEntry, NavLink } from "@/components/nav-main";
import { NavMain } from "@/components/nav-main";
import { NavUser } from "@/components/nav-user";
import { useSession } from "@/hooks/use-auth";
import type { Permission } from "@/lib/permissions";
import { PERMISSIONS, hasPermission } from "@/lib/permissions";

/** 設定的各個分頁。整組共用 `settings:read`，所以只在外層擋一次。 */
const SETTINGS_ITEMS: NavLink[] = [
  { title: "帳號與註冊", url: "/settings/account" },
  { title: "登入方式", url: "/settings/auth-providers" },
  { title: "密碼與雜湊", url: "/settings/password" },
  { title: "限流", url: "/settings/throttle" },
  { title: "寄信", url: "/settings/mail" },
  { title: "網路與代理", url: "/settings/network" },
  { title: "分享與取件", url: "/settings/share" },
  { title: "上傳設置", url: "/settings/upload" },
  { title: "存儲配置", url: "/settings/storage" },
  { title: "站台資訊", url: "/settings/site" },
  { title: "外觀", url: "/settings/theme" },
  { title: "功能開關", url: "/settings/features" },
];

/**
 * `permission` 為 undefined 代表只要登入就看得到。
 *
 * 這裡藏起來的項目只是不讓人點進一個會被轉走的頁面，真正的授權是每個路由的
 * `beforeLoad` 加上 server 端的檢查。
 */
const NAV_ITEMS: (NavEntry & { permission?: Permission })[] = [
  {
    icon: <IconDashboard />,
    permission: PERMISSIONS.statsRead,
    title: "儀表板",
    url: "/dashboard",
  },
  {
    icon: <IconFiles />,
    title: "我的分享",
    url: "/shares",
  },
  {
    icon: <IconUsers />,
    permission: PERMISSIONS.usersRead,
    title: "使用者",
    url: "/users",
  },
  {
    icon: <IconDatabase />,
    permission: PERMISSIONS.sharesReadAny,
    title: "全站分享",
    url: "/all-shares",
  },
  {
    icon: <IconKey />,
    permission: PERMISSIONS.invitesRead,
    title: "邀請碼",
    url: "/invites",
  },
  {
    icon: <IconShieldLock />,
    permission: PERMISSIONS.rolesRead,
    title: "角色與權限",
    url: "/roles",
  },
  {
    icon: <IconSettings />,
    items: SETTINGS_ITEMS,
    permission: PERMISSIONS.settingsRead,
    title: "系統設定",
  },
];

const DEV_ITEMS = [
  {
    icon: <IconArrowsSort />,
    title: "拖曳排序 Demo",
    url: "/dnd-demo",
  },
];

export function AppSidebar({ ...props }: React.ComponentProps<typeof Sidebar>) {
  const { data: account } = useSession();

  const items: NavEntry[] = NAV_ITEMS.filter(
    (item) => !item.permission || hasPermission(account, item.permission)
  );

  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              className="data-[slot=sidebar-menu-button]:p-1.5!"
              render={<Link aria-label="Tessera 首頁" to="/" />}
            >
              <IconInnerShadowTop className="size-5!" />
              <span className="text-base font-semibold">Tessera Admin</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={items} />
        <NavMain items={DEV_ITEMS} label="開發工具" />
      </SidebarContent>
      <SidebarFooter>
        <NavUser />
      </SidebarFooter>
    </Sidebar>
  );
}
