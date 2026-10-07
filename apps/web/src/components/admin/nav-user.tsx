import { IconDotsVertical, IconHome, IconLogout } from "@tabler/icons-react";
import { Link, useNavigate } from "@tanstack/react-router";
import { useLogout, useSession } from "@tessera/auth/hooks";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@tessera/ui/components/avatar";
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
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@tessera/ui/components/select";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@tessera/ui/components/sidebar";

import { accountRoleLabel, isUnassigned } from "@/lib/admin/permissions";
import { getLocale, setLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { locales } from "@/paraglide/runtime.js";

export function NavUser() {
  const { isMobile } = useSidebar();
  const { data: account } = useSession();
  const logout = useLogout();
  const navigate = useNavigate();
  const locale = getLocale();

  if (!account) {
    return null;
  }

  const onLogout = () => {
    logout.mutate(undefined, {
      onSuccess: () => navigate({ to: "/login" }),
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
              <AvatarImage alt={account.username} src="" />
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
                    <AvatarImage alt={account.username} src="" />
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
            {/* 回到首頁 */}
            <DropdownMenuItem
              render={<Link aria-label={m.app_sidebar_back_to_site()} to="/" />}
              className="justify-start"
            >
              <IconHome />
              {m.app_sidebar_back_to_site()}
            </DropdownMenuItem>
            <div className="px-2 py-1.5">
              <Select
                onValueChange={(value) => setLocale(value as typeof locale)}
                value={locale}
              >
                <SelectTrigger className="w-full">
                  <SelectValue placeholder={m.common_language()} />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {locales.map((availableLocale) => (
                      <SelectItem key={availableLocale} value={availableLocale}>
                        {availableLocale.toUpperCase()}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </div>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={logout.isPending} onClick={onLogout}>
              <IconLogout />
              {logout.isPending
                ? m.nav_user_logging_out()
                : m.nav_user_logout()}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}

function initials(name: string): string {
  return name.slice(0, 2).toUpperCase();
}
