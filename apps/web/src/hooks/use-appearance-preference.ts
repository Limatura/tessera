import { toast } from "@tessera/ui/components/toast";

import { usePreferences, useUpdatePreferences } from "@/hooks/use-preferences";
import type { Appearance } from "@/lib/appearance";
import { readSiteAppearance, resolveAppearance } from "@/lib/appearance";
import * as m from "@/paraglide/messages";

/**
 * The part the three appearance fields share: what to show as selected, and
 * what to send on click.
 *
 * It shows the resolved value (site default overridden by personal
 * preference), not the preference itself: someone who never chose should see
 * the colour they actually get as selected, not nothing selected.
 *
 * It sends the full `Appearance`: the endpoint is a PUT and omitted fields are
 * cleared as "no opinion", so changing one field must send the other two. Those
 * two come from the personal preference, not the resolved value, or changing
 * the accent would freeze "follow the site's mode" to whatever it is now.
 */
export function useAppearancePreference() {
  const { data: personal } = usePreferences();
  const update = useUpdatePreferences();

  const current = resolveAppearance(readSiteAppearance(), personal ?? {});

  const choose = (change: Appearance) => {
    update.mutate(
      { ...personal, ...change },
      {
        onError: () =>
          toast.add({ title: m.user_prefs_save_failed(), type: "error" }),
      }
    );
  };

  return { choose, current, isPending: update.isPending };
}
