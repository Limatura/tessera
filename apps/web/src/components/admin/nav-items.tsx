import {
  IconDashboard,
  IconDatabase,
  IconKey,
  IconSettings,
  IconShieldLock,
  IconUsers,
  IconWebhook,
} from "@tabler/icons-react";
import { useSession } from "@tessera/auth/hooks";
import type * as React from "react";

import type { Permission } from "@/lib/admin/permissions";
import { PERMISSIONS, hasPermission } from "@/lib/admin/permissions";
import * as m from "@/paraglide/messages";

export interface AdminNavItem {
  title: string;
  url: string;
  icon: React.ReactNode;
  permission: Permission;
}

export interface AdminNavGroup {
  title: string;
  items: AdminNavItem[];
}

/**
 * The admin pages, grouped the way the sidebar shows them. Shared with the
 * command menu so the two never list different pages.
 *
 * Every admin page needs a permission, so every item has one. Hiding an item
 * only spares the user a page that would redirect them. The real authorisation
 * is each route's `beforeLoad` plus the server's checks.
 *
 * The dozen or so settings tabs are not listed; the settings page switches them
 * with its own grouped menu, and the command menu adds them from
 * `settingsNavGroups()`.
 */
function adminNavGroups(): AdminNavGroup[] {
  return [
    {
      items: [
        {
          icon: <IconDashboard />,
          permission: PERMISSIONS.statsRead,
          title: m.nav_dashboard(),
          url: "/admin/dashboard",
        },
      ],
      title: m.admin_nav_group_overview(),
    },
    {
      items: [
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
      ],
      title: m.admin_nav_group_content(),
    },
    {
      items: [
        {
          icon: <IconShieldLock />,
          permission: PERMISSIONS.rolesRead,
          title: m.nav_roles(),
          url: "/admin/roles",
        },
        {
          icon: <IconWebhook />,
          permission: PERMISSIONS.settingsRead,
          title: m.nav_webhooks(),
          url: "/admin/webhooks",
        },
        {
          icon: <IconSettings />,
          permission: PERMISSIONS.settingsRead,
          title: m.nav_settings(),
          url: "/admin/settings",
        },
      ],
      title: m.admin_nav_group_manage(),
    },
  ];
}

/** The groups the signed-in user can open; a group left empty is dropped. */
export function useAdminNav(): AdminNavGroup[] {
  const { data: account } = useSession();
  return adminNavGroups()
    .map((group) => ({
      ...group,
      items: group.items.filter((item) =>
        hasPermission(account, item.permission)
      ),
    }))
    .filter((group) => group.items.length > 0);
}
