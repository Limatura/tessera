import { IconChevronRight } from "@tabler/icons-react";
import { Link, useRouterState } from "@tanstack/react-router";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@tessera/ui/components/collapsible";
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@tessera/ui/components/sidebar";

export interface NavLink {
  title: string;
  url: string;
  icon?: React.ReactNode;
}

/** 有 `items` 的是可展開的群組，本身不連到任何頁面。 */
export type NavEntry =
  | NavLink
  | { title: string; icon?: React.ReactNode; items: NavLink[] };

function isGroup(entry: NavEntry): entry is {
  title: string;
  icon?: React.ReactNode;
  items: NavLink[];
} {
  return "items" in entry;
}

function NavLinkItem({ item }: { item: NavLink }) {
  return (
    <SidebarMenuItem>
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
  );
}

/**
 * 一組收在下拉裡的頁面。
 *
 * 目前在哪一個子頁就預設展開，否則點進設定的某一頁、重整之後會看到群組是收起來的，
 * 得再點一次才知道自己在哪。
 */
function NavGroupItem({
  item,
}: {
  item: { title: string; icon?: React.ReactNode; items: NavLink[] };
}) {
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const open = item.items.some((sub) => pathname.startsWith(sub.url));

  return (
    <SidebarMenuItem>
      <Collapsible defaultOpen={open}>
        <SidebarMenuButton
          className="group/collapsible w-full"
          render={<CollapsibleTrigger />}
        >
          {item.icon}
          <span>{item.title}</span>
          <IconChevronRight className="ml-auto transition-transform duration-200 group-data-[panel-open]/collapsible:rotate-90" />
        </SidebarMenuButton>
        <CollapsibleContent>
          <SidebarMenuSub>
            {item.items.map((sub) => (
              <SidebarMenuSubItem key={sub.url}>
                <SidebarMenuSubButton
                  render={
                    <Link
                      activeProps={{ "data-active": true }}
                      aria-label={sub.title}
                      to={sub.url}
                    />
                  }
                >
                  <span>{sub.title}</span>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            ))}
          </SidebarMenuSub>
        </CollapsibleContent>
      </Collapsible>
    </SidebarMenuItem>
  );
}

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
          {items.map((item) =>
            isGroup(item) ? (
              <NavGroupItem item={item} key={item.title} />
            ) : (
              <NavLinkItem item={item} key={item.url} />
            )
          )}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}
