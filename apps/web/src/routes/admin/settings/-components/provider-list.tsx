import { IconChevronRight } from "@tabler/icons-react";
import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@tessera/ui/components/sheet";
import { useState } from "react";
import type * as React from "react";

import { ProviderIcon } from "@/components/provider-icon";
import * as m from "@/paraglide/messages";

import { isOauthConfigured, OauthProviderForm } from "./oauth-provider-card";
import { isOidcConfigured, OidcProviderForm } from "./oidc-provider-card";

type Settings = components["schemas"]["SettingsResponse"];

interface ProviderEntry {
  id: string;
  /** `ProviderIcon`'s provider code. */
  icon: string;
  title: string;
  description: string;
  configured: boolean;
  form: React.ReactNode;
}

/**
 * Adding a provider is one more entry here: the list and the side panel need no
 * change. Each entry keeps its own form and request (see the route's comment).
 */
function providerEntries(settings: Settings): ProviderEntry[] {
  return [
    {
      configured: isOidcConfigured(settings),
      description: m.settings_oidc_card_description(),
      form: <OidcProviderForm settings={settings} />,
      icon: "oidc",
      id: "oidc",
      title: "OIDC",
    },
    {
      configured: isOauthConfigured(settings, "auth.github"),
      description: m.settings_auth_providers_github_description(),
      form: (
        <OauthProviderForm
          baseUrlHint={m.settings_auth_providers_github_base_url_hint()}
          provider="auth.github"
          scopesHint={m.settings_auth_providers_github_scopes_hint()}
          settings={settings}
          title={m.settings_auth_providers_github_title()}
        />
      ),
      icon: "github",
      id: "github",
      title: m.settings_auth_providers_github_title(),
    },
    {
      configured: isOauthConfigured(settings, "auth.discord"),
      description: m.settings_auth_providers_discord_description(),
      form: (
        <OauthProviderForm
          provider="auth.discord"
          scopesHint={m.settings_auth_providers_discord_scopes_hint()}
          settings={settings}
          title={m.settings_auth_providers_discord_title()}
        />
      ),
      icon: "discord",
      id: "discord",
      title: m.settings_auth_providers_discord_title(),
    },
  ];
}

/**
 * One row per provider with its on/off status; a row opens that provider's form in a
 * side panel. The list stays one line per provider however many there are, and the
 * long forms only exist while open.
 *
 * `settings` is the loader's snapshot, so a row's status updates after the next fetch
 * of the settings, not the moment a form saves.
 */
export function ProviderList({ settings }: { settings: Settings }) {
  const [openId, setOpenId] = useState<string | null>(null);
  const entries = providerEntries(settings);
  const open = entries.find((entry) => entry.id === openId);

  return (
    <>
      <ul className="divide-y overflow-hidden rounded-xl border bg-card text-card-foreground">
        {entries.map((entry) => (
          <li key={entry.id}>
            <button
              className="flex w-full items-center gap-4 px-5 py-4 text-left transition-colors hover:bg-muted/50 focus-visible:bg-muted/50 focus-visible:outline-none"
              onClick={() => setOpenId(entry.id)}
              type="button"
            >
              <span className="flex size-10 shrink-0 items-center justify-center rounded-lg border bg-background">
                <ProviderIcon provider={entry.icon} size={20} />
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span className="font-medium text-sm">{entry.title}</span>
                <span className="truncate text-muted-foreground text-xs">
                  {entry.description}
                </span>
              </span>
              <Badge variant={entry.configured ? "default" : "outline"}>
                {entry.configured
                  ? m.settings_auth_providers_status_on()
                  : m.settings_auth_providers_status_off()}
              </Badge>
              <IconChevronRight
                aria-hidden="true"
                className="shrink-0 text-muted-foreground"
                size={16}
              />
            </button>
          </li>
        ))}
      </ul>

      <Sheet
        onOpenChange={(next) => {
          if (!next) {
            setOpenId(null);
          }
        }}
        open={open !== undefined}
      >
        {/* Full width on a phone: a long form in three quarters of 390px wraps
            every hint, and the strip of page left beside it does nothing. */}
        <SheetContent
          className="data-[side=right]:sm:max-w-xl data-[side=right]:max-md:w-full data-[side=right]:max-md:max-w-none data-[side=right]:max-md:border-l-0"
          side="right"
        >
          {open && (
            <>
              <SheetHeader className="border-b pt-[max(1rem,env(safe-area-inset-top))] pr-14 max-md:px-4 max-md:pb-3">
                <SheetTitle className="text-base">{open.title}</SheetTitle>
                <SheetDescription>{open.description}</SheetDescription>
              </SheetHeader>
              <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 md:px-6 md:pt-6">
                {open.form}
              </div>
            </>
          )}
        </SheetContent>
      </Sheet>
    </>
  );
}
