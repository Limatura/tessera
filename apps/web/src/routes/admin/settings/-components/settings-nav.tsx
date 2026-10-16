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

import type { SettingsNavGroup } from "@/lib/admin/settings-nav";
import { settingsNavGroups } from "@/lib/admin/settings-nav";
import * as m from "@/paraglide/messages";

/**
 * Grouped menu on the left of the settings pages. A sticky list on wide
 * screens, a select on narrow ones: thirteen tabs in a horizontally scrolling
 * row would push most of them off screen.
 */
export function SettingsNav() {
  const groups = settingsNavGroups();

  return (
    <>
      <nav
        aria-label={m.settings_page_shell_title()}
        className="sticky top-[calc(var(--header-height)+1.5rem)] hidden flex-col gap-5 self-start lg:flex"
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
