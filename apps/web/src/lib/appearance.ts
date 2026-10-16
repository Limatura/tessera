/**
 * Personal appearance preferences: parsing, merging with the site default, and
 * applying to `<html>`. The three enums live in `@tessera/ui/lib/appearance`,
 * next to the matching globals.css.
 */

import type { SiteAppearance } from "@tessera/ui/lib/appearance";
import {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  FALLBACK_APPEARANCE,
  memberOf,
} from "@tessera/ui/lib/appearance";

export {
  APPEARANCE_ACCENTS,
  APPEARANCE_MODES,
  APPEARANCE_THEMES,
  FALLBACK_APPEARANCE,
} from "@tessera/ui/lib/appearance";
export type {
  AccentColor,
  AppearanceMode,
  AppTheme,
  SiteAppearance,
} from "@tessera/ui/lib/appearance";

/**
 * What a user has chosen. Every field may be missing: missing means no opinion,
 * so it keeps following the site default, including after the owner changes
 * it. A full object filled with defaults cannot stand in for this, since it
 * would turn "no opinion" into "happened to pick the current default".
 */
export interface Appearance {
  mode?: SiteAppearance["mode"];
  theme?: SiteAppearance["theme"];
  accent?: SiteAppearance["accent"];
}

/**
 * Narrows anything (a server response, localStorage) to an `Appearance`.
 * Unknown fields are dropped instead of rejecting the object: these values end
 * up as HTML attributes, and a colour name stored by an older build that this
 * one removed should not make every preference unreadable.
 */
export function parseAppearance(value: unknown): Appearance {
  if (typeof value !== "object" || value === null) {
    return {};
  }
  const raw = value as Record<string, unknown>;
  const accent = memberOf(APPEARANCE_ACCENTS, raw.accent);
  const mode = memberOf(APPEARANCE_MODES, raw.mode);
  const theme = memberOf(APPEARANCE_THEMES, raw.theme);

  return {
    ...(accent ? { accent } : {}),
    ...(mode ? { mode } : {}),
    ...(theme ? { theme } : {}),
  };
}

/**
 * The site default the server writes onto `index.html`, read once at module
 * load. `applyAppearance` writes the same three attributes, so after the first
 * apply the DOM holds the resolved result, not the site default. Re-reading
 * each time would make clearing a preference fall back to the user's previous
 * choice instead of the owner's default, the one thing this mechanism exists
 * to provide.
 *
 * In development the vite dev server serves the raw index.html without these
 * attributes, so it falls back to `FALLBACK_APPEARANCE`.
 */
const SITE_APPEARANCE: SiteAppearance = readSiteAppearanceFromDocument();

function readSiteAppearanceFromDocument(): SiteAppearance {
  const root = document.documentElement;

  return {
    accent:
      memberOf(APPEARANCE_ACCENTS, root.dataset.accent) ??
      FALLBACK_APPEARANCE.accent,
    mode:
      memberOf(APPEARANCE_MODES, root.dataset.mode) ?? FALLBACK_APPEARANCE.mode,
    theme:
      memberOf(APPEARANCE_THEMES, root.dataset.theme) ??
      FALLBACK_APPEARANCE.theme,
  };
}

export function readSiteAppearance(): SiteAppearance {
  return SITE_APPEARANCE;
}

/** Personal preferences laid over the site default, field by field. */
export function resolveAppearance(
  site: SiteAppearance,
  personal: Appearance
): SiteAppearance {
  return {
    accent: personal.accent ?? site.accent,
    mode: personal.mode ?? site.mode,
    theme: personal.theme ?? site.theme,
  };
}

/**
 * Writes the resolved result onto `<html>`. `mode` is not handled here:
 * next-themes owns light and dark (the `system` media query and the `.dark`
 * class). This only touches `data-theme` and `data-accent`, different
 * attributes, so the two do not conflict.
 */
export function applyAppearance(resolved: SiteAppearance): void {
  const root = document.documentElement;
  root.dataset.theme = resolved.theme;
  root.dataset.accent = resolved.accent;
}
