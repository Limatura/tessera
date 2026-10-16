import * as m from "@/paraglide/messages";

export interface SettingsNavItem {
  title: string;
  url: string;
}

export interface SettingsNavGroup {
  title: string;
  items: SettingsNavItem[];
}

/**
 * Settings tabs grouped by what an administrator wants to change, not by server
 * key prefix. Retention is `share.*`, for example, but it belongs with who may
 * upload and how much, so it is on the uploads page.
 *
 * A function instead of a module-level const: the `m.xxx()` calls must read the
 * locale at render time.
 */
export function settingsNavGroups(): SettingsNavGroup[] {
  return [
    {
      items: [
        { title: m.nav_settings_site(), url: "/admin/settings/site" },
        { title: m.nav_settings_theme(), url: "/admin/settings/theme" },
        { title: m.nav_settings_og(), url: "/admin/settings/og" },
      ],
      title: m.settings_nav_group_general(),
    },
    {
      items: [
        { title: m.nav_settings_account(), url: "/admin/settings/account" },
        {
          title: m.nav_settings_auth_providers(),
          url: "/admin/settings/auth-providers",
        },
        { title: m.nav_settings_password(), url: "/admin/settings/password" },
      ],
      title: m.settings_nav_group_accounts(),
    },
    {
      items: [
        { title: m.nav_settings_upload(), url: "/admin/settings/upload" },
        { title: m.nav_settings_share(), url: "/admin/settings/share" },
        { title: m.nav_settings_storage(), url: "/admin/settings/storage" },
      ],
      title: m.settings_nav_group_files(),
    },
    {
      items: [
        { title: m.nav_settings_throttle(), url: "/admin/settings/throttle" },
        { title: m.nav_settings_network(), url: "/admin/settings/network" },
        { title: m.nav_settings_scan(), url: "/admin/settings/scan" },
      ],
      title: m.settings_nav_group_security(),
    },
    {
      items: [
        { title: m.nav_settings_mail(), url: "/admin/settings/mail" },
        { title: m.nav_settings_features(), url: "/admin/settings/features" },
        { title: m.nav_settings_reset(), url: "/admin/settings/reset" },
      ],
      title: m.settings_nav_group_system(),
    },
  ];
}
