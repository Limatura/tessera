import type { components } from "@tessera/api/schema";
import type * as React from "react";

import { useSettings } from "@/hooks/admin/use-settings";
import * as m from "@/paraglide/messages";

type Settings = components["schemas"]["SettingsResponse"];

/**
 * Load the settings first, then mount the form.
 *
 * The form reads its initial values once, on mount, so the data must exist by
 * then. If the form handled `undefined` itself, the first render's defaults
 * would be empty and pressing Save would wipe the real settings.
 */
export function SettingsLoader({
  children,
}: {
  children: (settings: Settings) => React.ReactNode;
}) {
  const settings = useSettings();

  if (settings.isPending) {
    return (
      <p className="text-muted-foreground text-sm">
        {m.settings_loader_loading()}
      </p>
    );
  }

  if (settings.isError || !settings.data) {
    return (
      <p className="text-destructive text-sm">{m.settings_loader_error()}</p>
    );
  }

  return children(settings.data);
}
