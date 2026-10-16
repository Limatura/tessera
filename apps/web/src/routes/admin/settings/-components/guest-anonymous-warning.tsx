import { IconAlertTriangle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/admin/use-settings";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

const GUEST_ROLE = "guest";

/**
 * The two settings live on different pages; `fixAt` names the other one, which this
 * page cannot fix: the default role on the accounts page, anonymous uploads on the
 * upload page.
 *
 * A function, not a module-level const: the `m.xxx()` calls must read the locale at
 * render time, and a const would freeze it at first module load.
 */
function fixTargets() {
  return {
    account: {
      action: m.settings_guest_warning_account_action(),
      label: m.settings_guest_warning_account_label(),
      to: "/admin/settings/account",
    },
    upload: {
      action: m.settings_guest_warning_upload_action(),
      label: m.settings_guest_warning_upload_label(),
      to: "/admin/settings/upload",
    },
  } as const;
}

/**
 * Warns when `auth.default_role` is `guest` while `features.anonymous_upload` is on.
 *
 * The two are on different pages, so both pages show it. Values come from the settings
 * in effect on the server, not form state, since it describes the instance as it is;
 * the page editing `default_role` can pass its unsaved value as `defaultRole` to
 * preview it.
 *
 * Why warn: guest has no permissions, but it restricts an account, not the instance.
 * With anonymous uploads on, the same person can sign out and upload as a stranger, so
 * the role restricts no one.
 */
export function GuestAnonymousWarning({
  defaultRole,
  fixAt,
}: {
  /** This page's unsaved `auth.default_role`; read from the server when absent. */
  defaultRole?: string;
  /** Where to fix it: the page holding the other setting. */
  fixAt: keyof ReturnType<typeof fixTargets>;
}) {
  const { data } = useSettings();

  const role = defaultRole ?? readSetting(data, "auth.default_role");
  const anonymousUpload = readSetting(data, "features.anonymous_upload");

  if (role !== GUEST_ROLE || anonymousUpload !== true) {
    return null;
  }

  const target = fixTargets()[fixAt];

  return (
    <SettingsNotice
      icon={<IconAlertTriangle />}
      title={m.settings_guest_warning_title()}
      tone="warning"
    >
      {m.settings_guest_warning_body_before()}
      <Link
        className="mx-1 underline underline-offset-4 hover:text-foreground"
        to={target.to}
      >
        {target.label}
      </Link>
      {m.settings_guest_warning_body_after({ action: target.action })}
    </SettingsNotice>
  );
}
