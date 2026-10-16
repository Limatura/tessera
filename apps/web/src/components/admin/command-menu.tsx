import {
  IconDeviceDesktop,
  IconHome,
  IconLanguage,
  IconMoon,
  IconPlus,
  IconSettings,
  IconSun,
} from "@tabler/icons-react";
import { useNavigate } from "@tanstack/react-router";
import { useSession } from "@tessera/auth/hooks";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@tessera/ui/components/command";
import { useEffect } from "react";

import { useAdminNav } from "@/components/admin/nav-items";
import { useThemeMode } from "@/hooks/admin/use-theme-mode";
import { PERMISSIONS, hasPermission } from "@/lib/admin/permissions";
import { settingsNavGroups } from "@/lib/admin/settings-nav";
import { LOCALE_LABELS, getLocale, setLocale } from "@/lib/locale";
import * as m from "@/paraglide/messages";
import { locales } from "@/paraglide/runtime.js";

/**
 * ⌘K / Ctrl+K: jump to any admin page or settings tab, or run one of a few
 * actions. Lists only what the signed-in account may open, from the same
 * sources as the sidebar and the settings menu, so the three never disagree.
 *
 * A settings item's search value also carries its group's name, so "security"
 * finds the throttle, network and scan tabs.
 */
export function CommandMenu({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const groups = useAdminNav();
  const { data: account } = useSession();
  const { setMode } = useThemeMode();
  const locale = getLocale();

  const canReadSettings = hasPermission(account, PERMISSIONS.settingsRead);
  const canCreateInvite = hasPermission(account, PERMISSIONS.invitesCreate);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        onOpenChange(!open);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onOpenChange]);

  const run = (command: () => void) => {
    onOpenChange(false);
    command();
  };

  return (
    <CommandDialog
      description={m.admin_command_description()}
      onOpenChange={onOpenChange}
      open={open}
      title={m.admin_command_title()}
    >
      <Command>
        <CommandInput placeholder={m.admin_command_placeholder()} />
        <CommandList>
          <CommandEmpty>{m.admin_command_empty()}</CommandEmpty>
          {groups.map((group) => (
            <CommandGroup heading={group.title} key={group.title}>
              {group.items.map((item) => (
                <CommandItem
                  key={item.url}
                  onSelect={() => run(() => navigate({ to: item.url }))}
                  value={item.title}
                >
                  {item.icon}
                  {item.title}
                </CommandItem>
              ))}
            </CommandGroup>
          ))}
          {canReadSettings && (
            <CommandGroup heading={m.nav_settings()}>
              {settingsNavGroups().flatMap((group) =>
                group.items.map((item) => (
                  <CommandItem
                    key={item.url}
                    onSelect={() => run(() => navigate({ to: item.url }))}
                    value={`${m.nav_settings()} ${group.title} ${item.title}`}
                  >
                    <IconSettings />
                    <span className="flex-1 truncate">{item.title}</span>
                    <span className="text-muted-foreground text-xs">
                      {group.title}
                    </span>
                  </CommandItem>
                ))
              )}
            </CommandGroup>
          )}
          <CommandSeparator />
          <CommandGroup heading={m.admin_command_group_actions()}>
            {canCreateInvite && (
              <CommandItem
                onSelect={() =>
                  run(() =>
                    navigate({ search: { create: true }, to: "/admin/invites" })
                  )
                }
              >
                <IconPlus />
                {m.invites_create_button()}
              </CommandItem>
            )}
            <CommandItem onSelect={() => run(() => setMode("light"))}>
              <IconSun />
              {m.user_prefs_theme_light()}
            </CommandItem>
            <CommandItem onSelect={() => run(() => setMode("dark"))}>
              <IconMoon />
              {m.user_prefs_theme_dark()}
            </CommandItem>
            <CommandItem onSelect={() => run(() => setMode("system"))}>
              <IconDeviceDesktop />
              {m.admin_command_theme_system()}
            </CommandItem>
            <CommandItem onSelect={() => run(() => navigate({ to: "/" }))}>
              <IconHome />
              {m.app_sidebar_back_to_site()}
            </CommandItem>
          </CommandGroup>
          <CommandGroup heading={m.common_language()}>
            {locales
              .filter((availableLocale) => availableLocale !== locale)
              .map((availableLocale) => (
                <CommandItem
                  key={availableLocale}
                  onSelect={() => run(() => setLocale(availableLocale))}
                  value={`${m.common_language()} ${LOCALE_LABELS[availableLocale]} ${availableLocale}`}
                >
                  <IconLanguage />
                  {LOCALE_LABELS[availableLocale]}
                </CommandItem>
              ))}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
