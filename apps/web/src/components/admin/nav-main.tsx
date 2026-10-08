import { Link } from "@tanstack/react-router";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@tessera/ui/components/sidebar";

export interface NavEntry {
  title: string;
  url: string;
  icon?: React.ReactNode;
}

/**
 * 側欄的一組連結。都是單層 —— 系統設定的分頁由設定頁自己的分組選單切換，不在這裡
 * 攤成一長串子項目。
 *
 * `Link` 的 active 判斷預設是前綴比對，所以在 `/admin/settings/mail` 時「系統設定」
 * 也是亮的。
 */
export function NavMain({
  items,
  label,
}: {
  items: NavEntry[];
  label?: string;
}) {
  if (items.length === 0) {
    return null;
  }

  return (
    <SidebarGroup>
      {label && <SidebarGroupLabel>{label}</SidebarGroupLabel>}
      <SidebarGroupContent className="flex flex-col gap-2">
        <SidebarMenu>
          {items.map((item) => (
            <SidebarMenuItem key={item.url}>
              <SidebarMenuButton
                tooltip={item.title}
                render={
                  <Link
                    activeProps={{ "data-active": true }}
                    aria-label={item.title}
                    to={item.url}
                  />
                }
              >
                {item.icon}
                <span>{item.title}</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}
