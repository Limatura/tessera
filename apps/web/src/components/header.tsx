import { IconLogin } from "@tabler/icons-react";
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

import { Logo } from "@/components/logo";
import { getLocale, setLocale } from "@/lib/locale";
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

    // 初始化狀態
    handleScroll();

    window.addEventListener("scroll", handleScroll, { passive: true });

    return () => {
      window.removeEventListener("scroll", handleScroll);
    };
  }, []);

  // 後台每一頁都要權限，一頁都進不去的人點連結只會被 admin 的 route guard 原地送回來，
  // 所以入口乾脆不出現。真正的授權由 server 擋，這裡只是不要給進不去的門。
  const showAdmin = canAccessAdmin(session);

  // 登出後回首頁,停在需要登入的頁面只會留下一個已經失效的畫面(#363)。
  const handleLogout = () => {
    logout.mutate(undefined, { onSuccess: () => navigate({ to: "/" }) });
  };

  return (
    // 內容(上傳卡片、結果頁)會捲到 Header 底下,沒有底色的話文字會直接疊在按鈕上,
    // 所以給一層毛玻璃把後面的東西霧化掉。
    <nav
      className={cn(
        "fixed z-50 w-full transition-[background-color,backdrop-filter] duration-300",
        isScrolled && "bg-background/50 backdrop-blur-xl"
      )}
    >
      {/* 手機寬度下 Logo(寬約 6 倍高)+ 語言 + 主題 + 登入加起來會超出視窗,把登入鈕擠出畫面,
          所以 Logo 在窄螢幕縮小、可再被壓縮,右側控制項固定不縮。 */}
      <div className="flex items-center justify-between gap-2 p-3 sm:p-4">
        <Link to="/" className="flex min-w-0 items-center">
          <Logo className="h-4 max-w-full sm:h-6" />
        </Link>
        <div className="flex shrink-0 items-center gap-2">
          <Select
            value={locale}
            onValueChange={(value) => setLocale(value as typeof locale)}
          >
            <SelectTrigger className="w-20 sm:w-24">
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
