import { IconAlertOctagon, IconInfoCircle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import type { ProviderRegistrationMode } from "@/lib/admin/settings-schema";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

/**
 * What a third-party sign-in does when the person has no account here yet.
 *
 * Two settings decide it together: this provider's `allow_registration`, and the
 * global `auth.registration`, consulted only when the former is `inherit` (see
 * `ProviderRegistration` in the server's `config.rs`). Describing only one gets
 * it wrong: `always` ignores the global setting and needs no invitation, while
 * `never` admits only existing accounts whatever the global setting says. Hence
 * one note per card instead of one per page.
 *
 * `mode` is the form value (what saving this card will do); the global setting
 * is the value in effect on the server. It lives on another page and cannot be
 * changed here, for the same reason as `GuestAnonymousWarning`.
 *
 * `openToWorld`: the text for a provider with no gate of its own (GitHub,
 * Discord, or OIDC with no group restriction). Combined with `always`, any
 * account at that provider can sign up here, so it gets the highest severity.
 * The wording differs (OIDC can fix it with a group, the other two cannot), so
 * the caller supplies it.
 */
export function ProviderRegistrationNote({
  mode,
  openToWorld,
}: {
  mode: ProviderRegistrationMode;
  openToWorld?: { title: string; body: string };
}) {
  const { data } = useSettings();
  const registration = readSetting(data, "auth.registration");

  if (mode === "always" && openToWorld) {
    return (
      <SettingsNotice
        icon={<IconAlertOctagon />}
        title={openToWorld.title}
        tone="danger"
      >
        {openToWorld.body}
      </SettingsNotice>
    );
  }

  if (mode === "always") {
    return (
      <SettingsNotice
        icon={<IconInfoCircle />}
        title={m.settings_provider_registration_headline_always()}
        tone="info"
      >
        {m.settings_provider_registration_detail_always()}
      </SettingsNotice>
    );
  }

  if (mode === "never") {
    return (
      <SettingsNotice
        icon={<IconInfoCircle />}
        title={m.settings_provider_registration_headline_never()}
        tone="info"
      >
        {m.settings_provider_registration_detail_never()}
      </SettingsNotice>
    );
  }

  return (
    <SettingsNotice
      icon={<IconInfoCircle />}
      title={headline(registration)}
      tone="info"
    >
      {detail(registration)} {m.settings_provider_registration_change_before()}
      <Link
        className="mx-1 underline underline-offset-4 hover:text-foreground"
        to="/admin/settings/account"
      >
        {m.settings_provider_registration_link_label()}
      </Link>
      {m.settings_provider_registration_change_after()}
    </SettingsNotice>
  );
}

/** Options for the "new accounts" select, shared by OIDC, GitHub and Discord. */
export function providerRegistrationOptions() {
  return [
    {
      label: m.settings_provider_registration_option_inherit(),
      value: "inherit",
    },
    {
      label: m.settings_provider_registration_option_always(),
      value: "always",
    },
    { label: m.settings_provider_registration_option_never(), value: "never" },
  ];
}

/**
 * Unknown values are treated as `open`, the server default: if the text must be
 * wrong, make it the likeliest one. A fourth value only appears when the server
 * gains a mode the panel does not know yet.
 */
function headline(registration: unknown): string {
  switch (registration) {
    case "closed": {
      return m.settings_provider_registration_headline_closed();
    }
    case "invite": {
      return m.settings_provider_registration_headline_invite();
    }
    default: {
      return m.settings_provider_registration_headline_open();
    }
  }
}

function detail(registration: unknown): string {
  switch (registration) {
    case "closed": {
      return m.settings_provider_registration_detail_closed();
    }
    case "invite": {
      return m.settings_provider_registration_detail_invite();
    }
    default: {
      return m.settings_provider_registration_detail_open();
    }
  }
}
