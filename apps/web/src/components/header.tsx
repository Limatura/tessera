import { IconLanguage, IconLogin } from "@tabler/icons-react";
import { Link, useNavigate } from "@tanstack/react-router";
import { useSession, useLogout } from "@tessera/auth/hooks";
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
import { useEffect, useState } from "react";

import { InstallAppButton } from "@/components/install-app-button";
import { Logo } from "@/components/logo";
import { LOCALE_LABELS, getLocale, setLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { locales } from "@/paraglide/runtime.js";

export function Header() {
  const locale = getLocale();
  const { data: session } = useSession();
  const logout = useLogout();
  const navigate = useNavigate();

  const [isScrolled, setIsScrolled] = useState(false);

  useEffect(() => {
    const handleScroll = () => {
      setIsScrolled(window.scrollY > 10);
    };

    // Set the initial state
    handleScroll();

    window.addEventListener("scroll", handleScroll, { passive: true });

    return () => {
      window.removeEventListener("scroll", handleScroll);
    };
  }, []);

  // Every admin page needs a permission. Someone who can open none of them would
  // only be bounced back by the admin route guard, so the entry is hidden. The
  // server does the real authorisation.
  const showAdmin = canAccessAdmin(session);

  // Go home after signing out; staying on a page that needs a session would leave a dead screen.
  const handleLogout = () => {
    logout.mutate(undefined, { onSuccess: () => navigate({ to: "/" }) });
  };

  return (
    // Content (the upload card, the result page) scrolls under the header, and
    // without a background its text would sit on the buttons, so blur it.
    <nav
      className={cn(
        "fixed z-50 w-full transition-[background-color,backdrop-filter] duration-300",
        isScrolled && "bg-background/50 backdrop-blur-xl"
      )}
    >
      <div className="flex items-center justify-between gap-2 p-3 sm:p-4">
        <Link to="/" className="flex shrink-0 items-center">
          <Logo className="size-8" />
        </Link>
        <div className="flex shrink-0 items-center gap-2">
          <Select
            value={locale}
            onValueChange={(value) => setLocale(value as typeof locale)}
          >
            <SelectTrigger
              aria-label={m.common_language()}
              className="size-7 justify-center px-0 sm:w-32 sm:justify-between sm:px-2 max-sm:[&>svg:last-child]:hidden!"
            >
              <IconLanguage className="size-4 shrink-0" />
              <SelectValue
                className="max-sm:hidden!"
                placeholder={m.common_language()}
              >
                {(value: typeof locale) => LOCALE_LABELS[value]}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {locales.map((availableLocale) => (
                  <SelectItem key={availableLocale} value={availableLocale}>
                    {LOCALE_LABELS[availableLocale]}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
          <InstallAppButton />
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
                  render={<Link to="/user" search={{ tab: "preferences" }} />}
                >
                  {m.user_tab_preferences()}
                </DropdownMenuItem>
                <DropdownMenuItem
                  render={<Link search={{ page: 1 }} to="/shares" />}
                >
                  {m.shares_page_title()}
                </DropdownMenuItem>
                {showAdmin && (
                  <DropdownMenuItem render={<Link to="/admin" />}>
                    {m.common_admin_console()}
                  </DropdownMenuItem>
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
