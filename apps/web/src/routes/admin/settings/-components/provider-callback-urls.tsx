import { IconAlertTriangle, IconCheck, IconCopy } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";
import { useState } from "react";

import { copyText } from "@/lib/copy-text";
import * as m from "@/paraglide/messages";

import { SettingsNotice } from "./form-fields";

/**
 * The addresses to paste into the provider's console.
 *
 * A provider sends the browser back only to a registered redirect URI, and one
 * wrong character means `refused`, shown on their page instead of here.
 * Operators used to assemble these by hand from the rules in
 * `auth/oidc/flow.rs`.
 *
 * Built from `server.public_url`, not the browser's current address: the former
 * is what goes to the provider (see `redirect_uri` in `flow.rs`). The panel often
 * runs on another domain or port, and `window.location` would give a value that
 * looks right but fails at sign-in.
 */
export function ProviderCallbackUrls({
  publicUrl,
  provider,
  showBackchannel = false,
}: {
  /** Current `server.public_url`. Nothing can be built when it is empty. */
  publicUrl: string;
  /** The path segment: `oidc`, `github` or `discord`. */
  provider: string;
  /**
   * Only when discovery was read and the provider advertises support. Few do,
   * so showing it unconditionally is noise.
   */
  showBackchannel?: boolean;
}) {
  const base = publicUrl.trim().replace(/\/+$/u, "");

  if (base === "") {
    return <NoPublicUrl />;
  }

  return (
    <div className="space-y-3 rounded-md border border-dashed p-3">
      <p className="text-muted-foreground text-sm">
        {m.settings_provider_callback_intro()}
      </p>
      <CopyableUrl
        hint={m.settings_provider_callback_redirect_hint()}
        label={m.settings_provider_callback_redirect_label()}
        url={`${base}/api/v1/auth/${provider}/callback`}
      />
      {showBackchannel ? (
        <CopyableUrl
          hint={m.settings_provider_callback_backchannel_hint()}
          label="Back-channel logout URL"
          url={`${base}/api/v1/auth/oidc/backchannel-logout`}
        />
      ) : null}
    </div>
  );
}

/**
 * Without `server.public_url` nothing can be built, and sign-in itself fails
 * too (`flow.rs` returns `NoPublicUrl`), so say "set it first" instead of
 * showing half an address.
 */
function NoPublicUrl() {
  return (
    <SettingsNotice
      icon={<IconAlertTriangle />}
      title={m.settings_provider_callback_no_public_url_title()}
      tone="warning"
    >
      {m.settings_provider_callback_no_public_url_description()}{" "}
      <Link
        className="underline underline-offset-4 hover:text-foreground"
        to="/admin/settings/network"
      >
        {m.nav_settings_network()}
      </Link>
    </SettingsNotice>
  );
}

function CopyableUrl({
  label,
  url,
  hint,
}: {
  label: string;
  url: string;
  hint: string;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    // On failure the address is still on screen to select and copy.
    if (await copyText(url)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-sm">{label}</span>
        <Button onClick={handleCopy} size="sm" type="button" variant="ghost">
          {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
          {copied
            ? m.settings_provider_callback_copied()
            : m.settings_provider_callback_copy()}
        </Button>
      </div>
      {/* break-all: unbroken text would add a horizontal scrollbar on narrow
         screens. <code> instead of a read-only input, since the point is
         reading and copying, not editing. */}
      <code className="block break-all rounded bg-muted px-2 py-1.5 font-mono text-xs">
        {url}
      </code>
      <p className="text-muted-foreground text-xs">{hint}</p>
    </div>
  );
}
