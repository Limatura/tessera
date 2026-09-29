import { IconLogin } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { canAccessAdmin } from "@tessera/auth/permissions";
import { AnimatedThemeToggler } from "@tessera/ui/components/animated-theme-toggler";
import {
  Avatar,
  AvatarFallback,
  AvatarImage,
} from "@tessera/ui/components/avatar";
import { buttonVariants } from "@tessera/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
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
import { cn } from "@tessera/ui/lib/utils";

import { Logo } from "@/components/logo";
import { useSession, useLogout } from "@/hooks/use-auth";
import { getLocale, setLocale } from "@/lib/locale";
import { adminUrl } from "@/lib/web-url";
import * as m from "@/paraglide/messages";
import { locales } from "@/paraglide/runtime.js";

export function Header() {
  const locale = getLocale();
  const { data: session } = useSession();
  const logout = useLogout();

  // 後台每一頁都要權限，一頁都進不去的人點連結只會被 admin 的 route guard 原地送回來，
  // 所以入口乾脆不出現。真正的授權由 server 擋，這裡只是不要給進不去的門。
  const adminHref = canAccessAdmin(session) ? adminUrl()?.toString() : null;

  const handleLogout = () => {
    logout.mutate();
  };

  return (
    <nav className="fixed w-full z-50">
      <div className="flex items-center justify-between gap-2 p-4">
        <Link to="/" className="flex items-center">
          <Logo className="h-6" />
        </Link>
        <div className="flex items-center gap-2">
          <Select
            value={locale}
            onValueChange={(value) => setLocale(value as typeof locale)}
          >
            <SelectTrigger className="w-24">
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
          <AnimatedThemeToggler
            className={buttonVariants({ size: "icon", variant: "outline" })}
          />
          {session ? (
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <button
                    type="button"
                    className="rounded-full focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                  >
                    <Avatar size="default">
                      <AvatarImage src="" alt={session.username} />
                      <AvatarFallback>
                        {getInitials(session.username)}
                      </AvatarFallback>
                    </Avatar>
                  </button>
                }
              />
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  render={<Link to="/user" search={{ tab: "account" }} />}
                >
                  {m.user_tab_account()}
                </DropdownMenuItem>
                <DropdownMenuItem
                  render={<Link search={{ page: 1 }} to="/shares" />}
                >
                  {m.shares_page_title()}
                </DropdownMenuItem>
                <DropdownMenuItem
                  render={<Link to="/user" search={{ tab: "preferences" }} />}
                >
                  {m.user_tab_preferences()}
                </DropdownMenuItem>
                {adminHref && (
                  <DropdownMenuItem
                    render={<a href={adminHref}>{m.common_admin_console()}</a>}
                  />
                )}
                <DropdownMenuItem variant="destructive" onClick={handleLogout}>
                  {m.common_logout()}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : (
            <Link
              to="/login"
              className={cn(
                buttonVariants({ size: "icon", variant: "outline" })
              )}
            >
              <IconLogin size={20} />
            </Link>
          )}
        </div>
      </div>
    </nav>
  );
}

function getInitials(username: string): string {
  return username.charAt(0).toUpperCase();
}
