import { IconDotsVertical, IconLogout } from "@tabler/icons-react";
import { Avatar, AvatarFallback } from "@tessera/ui/components/avatar";
import { Badge } from "@tessera/ui/components/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@tessera/ui/components/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@tessera/ui/components/sidebar";

import { useLogout, useSession } from "@/hooks/use-auth";
import { accountRoleLabel, isUnassigned } from "@/lib/permissions";
import { loginUrl } from "@/lib/web-url";

function initials(name: string): string {
  return name.slice(0, 2).toUpperCase();
}

export function NavUser() {
  const { isMobile } = useSidebar();
  const { data: account } = useSession();
  const logout = useLogout();

  if (!account) {
    return null;
  }

  const onLogout = () => {
    logout.mutate(undefined, {
      // 登入頁在 apps/web，不在這包 SPA 的 router 裡，只能整頁跳轉。
      onSuccess: () => {
        window.location.assign(loginUrl().toString());
      },
    });
  };

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton className="aria-expanded:bg-muted" size="lg" />
            }
          >
            <Avatar className="size-8 rounded-lg grayscale">
              <AvatarFallback className="rounded-lg">
                {initials(account.username)}
              </AvatarFallback>
            </Avatar>
            <div className="grid flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">{account.username}</span>
              <span className="truncate text-xs text-foreground/70">
                {account.email}
              </span>
            </div>
            <IconDotsVertical className="ml-auto size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="end"
            className="min-w-56"
            side={isMobile ? "bottom" : "right"}
            sideOffset={4}
          >
            <DropdownMenuGroup>
              <DropdownMenuLabel className="p-0 font-normal">
                <div className="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
                  <Avatar className="size-8">
                    <AvatarFallback className="rounded-lg">
                      {initials(account.username)}
                    </AvatarFallback>
                  </Avatar>
                  <div className="grid flex-1 text-left text-sm leading-tight">
                    <span className="truncate font-medium">
                      {account.username}
                    </span>
                    <span className="truncate text-xs text-muted-foreground">
                      {account.email}
                    </span>
                  </div>
                </div>
              </DropdownMenuLabel>
              <div className="px-2 pb-1.5">
                <Badge
                  variant={isUnassigned(account) ? "outline" : "secondary"}
                >
                  {accountRoleLabel(account)}
                </Badge>
              </div>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={logout.isPending} onClick={onLogout}>
              <IconLogout />
              {logout.isPending ? "登出中…" : "登出"}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
