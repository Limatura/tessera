import {
  IconArrowsSort,
  IconDashboard,
  IconFiles,
  IconInnerShadowTop,
  IconKey,
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

import { NavMain } from "@/components/nav-main";
import { NavUser } from "@/components/nav-user";

const data = {
  navDev: [
    {
      icon: <IconArrowsSort />,
      title: "拖曳排序 Demo",
      url: "/dnd-demo",
    },
  ],
  navMain: [
    {
      icon: <IconDashboard />,
      title: "儀表板",
      url: "/dashboard",
    },
    {
      icon: <IconFiles />,
      title: "分享管理",
      url: "/shares",
    },
    {
      icon: <IconUsers />,
      title: "使用者",
      url: "/users",
    },
    {
      icon: <IconKey />,
      title: "邀請碼",
      url: "/invites",
    },
    {
      icon: <IconShieldLock />,
      title: "角色與權限",
      url: "/roles",
    },
  ],
  user: {
    avatar: "",
    email: "admin@localhost.invalid",
    name: "admin",
  },
};

export function AppSidebar({ ...props }: React.ComponentProps<typeof Sidebar>) {
  return (
    <Sidebar collapsible="offcanvas" {...props}>
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              className="data-[slot=sidebar-menu-button]:p-1.5!"
              render={<Link to="/" aria-label="Tessera 首頁" />}
            >
              <IconInnerShadowTop className="size-5!" />
              <span className="text-base font-semibold">Tessera Admin</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={data.navMain} />
        <NavMain items={data.navDev} label="開發工具" />
      </SidebarContent>
      <SidebarFooter>
        <NavUser user={data.user} />
      </SidebarFooter>
    </Sidebar>
  );
}
