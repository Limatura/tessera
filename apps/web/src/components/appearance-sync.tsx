import { useSession } from "@tessera/auth/hooks";
import { useTheme } from "next-themes";
import { useEffect } from "react";

import { usePreferences } from "@/hooks/use-preferences";
import {
  applyAppearance,
  readSiteAppearance,
  resolveAppearance,
} from "@/lib/appearance";

/**
 * Resolves the site default plus this user's preferences and writes the result
 * to `<html>`. Renders nothing; it is a side effect mounted once in __root.
 *
 * No inline script in index.html: the server already writes the site default
 * on `<html>` (assets.rs), so the first paint has the right colours and there
 * is no flash to prevent. This only applies the signed-in user's preferences
 * over the default, which has to wait for the session query anyway.
 *
 * Anonymous visitors: `usePreferences` is disabled without a session, so
 * `personal` is empty and the result equals the site default the server
 * already wrote. Writing it again is a no-op.
 */
export function AppearanceSync() {
  const { data: session } = useSession();
  const isSignedIn = Boolean(session);
  // While a password change is forced the preferences endpoint returns 403, so
  // use the site default; it refetches once the session changes.
  const { data: personal } = usePreferences(
    isSignedIn && !session?.must_change_password
  );
  const { setTheme } = useTheme();

  const mode = personal?.mode;
  const theme = personal?.theme;
  const accent = personal?.accent;

  useEffect(() => {
    const site = readSiteAppearance();
    applyAppearance(
      resolveAppearance(site, isSignedIn ? { accent, mode, theme } : {})
    );
  }, [accent, isSignedIn, mode, theme]);

  /*
   * Light/dark goes through next-themes, since `system` must keep listening to
   * prefers-color-scheme and it already does. Push a value only if the user
   * chose one; otherwise leave its localStorage value alone, or every load
   * would overwrite a toggle made elsewhere.
   */
  useEffect(() => {
    if (isSignedIn && mode) {
      setTheme(mode);
    }
  }, [isSignedIn, mode, setTheme]);

  return null;
}
