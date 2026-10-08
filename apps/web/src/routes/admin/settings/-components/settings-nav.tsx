import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import { cn } from "@tessera/ui/lib/utils";

import * as m from "@/paraglide/messages";

interface SettingsNavItem {
  title: string;
  url: string;
}

interface SettingsNavGroup {
  title: string;
  items: SettingsNavItem[];
}

/**
 * 設定分頁的分組。照管理者「想改什麼」分，而不是照 server 的 key 前綴分 ——
 * 例如保存期限是 `share.*`，但它跟「誰能上傳、能傳多大」是同一件事，放在上傳頁。
 *
 * 是函式而不是 module-level const —— 裡面呼叫的 `m.xxx()` 要在渲染當下讀 locale。
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
      ],
      title: m.settings_nav_group_security(),
    },
    {
      items: [
        { title: m.nav_settings_mail(), url: "/admin/settings/mail" },
        { title: m.nav_settings_features(), url: "/admin/settings/features" },
      ],
      title: m.settings_nav_group_system(),
    },
  ];
}

/**
 * 設定頁左側的分組選單。寬螢幕是一欄跟著捲動的清單，窄螢幕換成下拉選單 ——
 * 十三個分頁排成橫向捲動列的話，大部分會被推到畫面外看不到。
 */
export function SettingsNav() {
  const groups = settingsNavGroups();

  return (
    <>
      <nav
        aria-label={m.settings_page_shell_title()}
        className="sticky top-6 hidden flex-col gap-5 self-start lg:flex"
      >
        {groups.map((group) => (
          <div className="flex flex-col gap-1" key={group.title}>
            <p className="px-2 font-medium text-muted-foreground text-xs">
              {group.title}
            </p>
            <ul className="flex flex-col gap-0.5">
              {group.items.map((item) => (
                <li key={item.url}>
                  <Link
                    activeProps={{
                      "aria-current": "page",
                      className: "bg-muted font-medium text-foreground",
                    }}
                    className={cn(
                      "block rounded-md px-2 py-1.5 text-muted-foreground text-sm transition-colors",
                      "hover:bg-muted/60 hover:text-foreground",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    )}
                    to={item.url}
                  >
                    {item.title}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
      <SettingsNavSelect groups={groups} />
    </>
  );
}

function SettingsNavSelect({ groups }: { groups: SettingsNavGroup[] }) {
  const navigate = useNavigate();
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const items = groups.flatMap((group) => group.items);
  const current = items.find((item) => pathname.startsWith(item.url));

  return (
    <div className="lg:hidden">
      <Select
        onValueChange={(url) => {
          if (url) {
            navigate({ to: url });
          }
        }}
        value={current?.url ?? null}
      >
        <SelectTrigger
          aria-label={m.settings_page_shell_title()}
          className="w-full"
        >
          <SelectValue>
            {(url: string) => items.find((item) => item.url === url)?.title}
          </SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          {groups.map((group) => (
            <SelectGroup key={group.title}>
              <SelectLabel>{group.title}</SelectLabel>
              {group.items.map((item) => (
                <SelectItem key={item.url} value={item.url}>
                  {item.title}
                </SelectItem>
              ))}
            </SelectGroup>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
