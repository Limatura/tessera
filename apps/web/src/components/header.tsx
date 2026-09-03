import { IconLogin } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
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
import * as m from "@/paraglide/messages";
import { locales } from "@/paraglide/runtime.js";

function getInitials(username: string): string {
  return username.charAt(0).toUpperCase();
}

export function Header() {
  const locale = getLocale();
  const { data: session } = useSession();
  const logout = useLogout();

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
