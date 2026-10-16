/**
 * The three appearance enums.
 *
 * These lists must match three places: `Appearance` in
 * `crates/tessera-core/src/appearance.rs` (PUT /me/preferences and
 * PUT /admin/settings return 400 for anything else), this package's
 * `styles/globals.css` (each value has hardcoded CSS), and the server's
 * `Config::default()` (the fallback below). An eighth colour means changing all
 * three.
 *
 * Kept in `@tessera/ui` because it is tied to globals.css: a value changed
 * without the CSS shows up as an option that can be picked but does nothing.
 * In the same package that is harder to miss.
 */

/** `appearance.mode` */
export const APPEARANCE_MODES = ["light", "dark", "system"] as const;

/** `appearance.theme`: neutral palettes. */
export const APPEARANCE_THEMES = [
  "default",
  "graphite",
  "sepia",
  "matsu",
  "cosmic",
  "slate",
  "vintage",
] as const;

/** `appearance.accent`: accent colours. */
export const APPEARANCE_ACCENTS = [
  "violet",
  "rose",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
] as const;

export type AppearanceMode = (typeof APPEARANCE_MODES)[number];
export type AppTheme = (typeof APPEARANCE_THEMES)[number];
export type AccentColor = (typeof APPEARANCE_ACCENTS)[number];

/** The site default the server writes into `index.html`: what someone with no preference sees. */
export interface SiteAppearance {
  mode: AppearanceMode;
  theme: AppTheme;
  accent: AccentColor;
}

/**
 * The last resort when none of the three sources has a value. Same as the
 * server's `Config::default()`.
 */
export const FALLBACK_APPEARANCE: SiteAppearance = {
  accent: "orange",
  mode: "system",
  theme: "default",
};

/**
 * The value if recognised, otherwise `undefined`; the caller decides whether to
 * fall back to a default.
 */
export function memberOf<T extends string>(
  allowed: readonly T[],
  value: unknown
): T | undefined {
  return typeof value === "string" &&
    (allowed as readonly string[]).includes(value)
    ? (value as T)
    : undefined;
}
