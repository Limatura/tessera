import {
  IconDashboard,
  IconListDetails,
  IconChartBar,
  IconFolder,
  IconUsers,
  IconCamera,
  IconFileDescription,
  IconFileAi,
  IconSettings,
  IconHelp,
  IconSearch,
  IconDatabase,
  IconReport,
  IconFileWord,
  IconInnerShadowTop,
} from "@tabler/icons-react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@tessera/ui/components/sidebar";
import * as React from "react";

import { NavDocuments } from "@/components/nav-documents";
import { NavMain } from "@/components/nav-main";
import { NavSecondary } from "@/components/nav-secondary";
import { NavUser } from "@/components/nav-user";

const data = {
  documents: [
    {
      icon: <IconDatabase />,
      name: "Data Library",
      url: "#",
    },
    {
      icon: <IconReport />,
      name: "Reports",
      url: "#",
    },
    {
      icon: <IconFileWord />,
      name: "Word Assistant",
      url: "#",
    },
  ],
  navClouds: [
    {
      icon: <IconCamera />,
      isActive: true,
      items: [
        {
          title: "Active Proposals",
          url: "#",
        },
        {
          title: "Archived",
          url: "#",
        },
      ],
      title: "Capture",
      url: "#",
    },
    {
      icon: <IconFileDescription />,
      items: [
        {
          title: "Active Proposals",
          url: "#",
        },
        {
          title: "Archived",
          url: "#",
        },
      ],
      title: "Proposal",
      url: "#",
    },
    {
      icon: <IconFileAi />,
      items: [
        {
          title: "Active Proposals",
          url: "#",
        },
        {
          title: "Archived",
          url: "#",
        },
      ],
      title: "Prompts",
      url: "#",
    },
  ],
  navMain: [
    {
      icon: <IconDashboard />,
      title: "Dashboard",
      url: "#",
    },
    {
      icon: <IconListDetails />,
      title: "Lifecycle",
      url: "#",
    },
    {
      icon: <IconChartBar />,
      title: "Analytics",
      url: "#",
    },
    {
      icon: <IconFolder />,
      title: "Projects",
      url: "#",
    },
    {
      icon: <IconUsers />,
      title: "Team",
      url: "#",
    },
  ],
  navSecondary: [
    {
      icon: <IconSettings />,
      title: "Settings",
      url: "#",
    },
    {
      icon: <IconHelp />,
      title: "Get Help",
      url: "#",
    },
    {
      icon: <IconSearch />,
      title: "Search",
      url: "#",
    },
  ],
  user: {
    avatar: "/avatars/shadcn.jpg",
    email: "m@example.com",
    name: "shadcn",
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
              render={<a href="/" aria-label="Acme Inc. home" />}
            >
              <IconInnerShadowTop className="size-5!" />
              <span className="text-base font-semibold">Acme Inc.</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={data.navMain} />
        <NavDocuments items={data.documents} />
        <NavSecondary items={data.navSecondary} className="mt-auto" />
      </SidebarContent>
      <SidebarFooter>
        <NavUser user={data.user} />
      </SidebarFooter>
    </Sidebar>
  );
}
