import { IconChevronLeft, IconSearch } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { AnimatedThemeToggler } from "@tessera/ui/components/animated-theme-toggler";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@tessera/ui/components/breadcrumb";
import { Button, buttonVariants } from "@tessera/ui/components/button";
import { Separator } from "@tessera/ui/components/separator";
import { SidebarTrigger } from "@tessera/ui/components/sidebar";
import { cn } from "@tessera/ui/lib/utils";
import { Fragment, useState } from "react";

import { CommandMenu } from "@/components/admin/command-menu";
import { useThemeMode } from "@/hooks/admin/use-theme-mode";
import * as m from "@/paraglide/messages";

const APPLE_PLATFORM = /Mac|iPhone|iPad/u;

export interface Crumb {
  title: string;
  /** Where the crumb links to; the last crumb is the current page and is not a link. */
  to: string;
}

/**
 * Sticky title bar: the sidebar toggle, where you are, then search and the
 * light/dark switch.
 *
 * Desktop shows the whole trail ("Settings › Mail"). A phone has no room for
 * it, so there it is a back link to the parent and the current page's title.
 */
export function SiteHeader({ crumbs }: { crumbs: Crumb[] }) {
  const [searchOpen, setSearchOpen] = useState(false);
  const current = crumbs.at(-1);
  const parent = crumbs.at(-2);

  return (
    <header className="sticky top-0 z-20 flex h-(--header-height) shrink-0 items-center border-b bg-background/95 transition-[width,height] ease-linear supports-[backdrop-filter]:bg-background/80 supports-[backdrop-filter]:backdrop-blur md:rounded-t-xl">
      <div className="flex w-full min-w-0 items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1 max-md:size-9" />
        <Separator
          className="mx-2 h-4 data-vertical:self-auto"
          orientation="vertical"
        />

        <div className="flex min-w-0 flex-1 items-center gap-1 md:hidden">
          {parent && (
            <Link
              aria-label={parent.title}
              className={cn(
                buttonVariants({ size: "icon", variant: "ghost" }),
                "-ml-1 size-9 shrink-0"
              )}
              to={parent.to}
            >
              <IconChevronLeft />
            </Link>
          )}
          <h1 className="truncate font-heading font-medium text-base">
            {current?.title}
          </h1>
        </div>

        <Breadcrumb className="hidden min-w-0 flex-1 md:block">
          <BreadcrumbList>
            {crumbs.map((crumb, index) => (
              <Fragment key={crumb.to}>
                {index > 0 && <BreadcrumbSeparator />}
                <BreadcrumbItem>
                  {index === crumbs.length - 1 ? (
                    <BreadcrumbPage className="font-heading font-medium text-base">
                      <h1>{crumb.title}</h1>
                    </BreadcrumbPage>
                  ) : (
                    <BreadcrumbLink
                      className="text-base"
                      render={<Link to={crumb.to} />}
                    >
                      {crumb.title}
                    </BreadcrumbLink>
                  )}
                </BreadcrumbItem>
              </Fragment>
            ))}
          </BreadcrumbList>
        </Breadcrumb>

        <div className="flex shrink-0 items-center gap-1 md:gap-2">
          <SearchButton onClick={() => setSearchOpen(true)} />
          <ThemeToggle />
        </div>
      </div>
      <CommandMenu onOpenChange={setSearchOpen} open={searchOpen} />
    </header>
  );
}

/** A search box on desktop with the shortcut shown; an icon button on a phone. */
function SearchButton({ onClick }: { onClick: () => void }) {
  return (
    <>
      <Button
        aria-label={m.admin_command_title()}
        className="size-9 md:hidden"
        onClick={onClick}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconSearch />
      </Button>
      <Button
        className="hidden w-48 justify-start gap-2 text-muted-foreground md:inline-flex lg:w-56"
        onClick={onClick}
        type="button"
        variant="outline"
      >
        <IconSearch />
        <span className="flex-1 text-left">{m.admin_command_title()}</span>
        <kbd className="pointer-events-none rounded border bg-muted px-1.5 font-mono text-xs text-muted-foreground">
          {isMac() ? "⌘K" : "Ctrl K"}
        </kbd>
      </Button>
    </>
  );
}

function ThemeToggle() {
  const { resolved, setMode } = useThemeMode();
  return (
    <AnimatedThemeToggler
      aria-label={m.admin_toggle_theme()}
      className={cn(
        buttonVariants({ size: "icon", variant: "ghost" }),
        "max-md:size-9"
      )}
      onThemeChange={setMode}
      theme={resolved}
    />
  );
}

function isMac(): boolean {
  return APPLE_PLATFORM.test(navigator.platform);
}
