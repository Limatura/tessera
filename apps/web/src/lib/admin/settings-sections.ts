/**
 * Which settings page a dotted key is edited on, for the "Reset to defaults"
 * page to group what was changed. `null` for a key no page edits yet (speed
 * test, first-run admin); those are grouped as "other".
 *
 * Rules are checked in order, first match wins, so a specific key comes
 * before its prefix (`share.default_expiry_hours` is on the uploads page, the
 * rest of `share.*` on the sharing page).
 */
const RULES: [match: string, url: string][] = [
  ["appearance.", "/admin/settings/theme"],
  ["site.", "/admin/settings/site"],
  ["og.", "/admin/settings/og"],
  ["auth.oidc.", "/admin/settings/auth-providers"],
  ["auth.github.", "/admin/settings/auth-providers"],
  ["auth.discord.", "/admin/settings/auth-providers"],
  ["auth.registration", "/admin/settings/account"],
  ["auth.default_role", "/admin/settings/account"],
  ["auth.session_ttl_hours", "/admin/settings/account"],
  ["auth.login_history_days", "/admin/settings/account"],
  ["auth.hashing.", "/admin/settings/password"],
  ["auth.password.", "/admin/settings/password"],
  ["auth.pwned.", "/admin/settings/password"],
  ["auth.throttle.", "/admin/settings/throttle"],
  ["auth.cookie_secure", "/admin/settings/network"],
  ["server.public_url", "/admin/settings/network"],
  ["server.request_timeout_secs", "/admin/settings/network"],
  ["server.trusted_proxies", "/admin/settings/network"],
  ["server.upload_stall_secs", "/admin/settings/network"],
  ["server.part_size_bytes", "/admin/settings/storage"],
  ["features.anonymous_upload", "/admin/settings/upload"],
  ["share.default_expiry_hours", "/admin/settings/upload"],
  ["share.max_expiry_hours", "/admin/settings/upload"],
  ["storage.max_total_bytes", "/admin/settings/upload"],
  ["features.encryption", "/admin/settings/share"],
  ["share.", "/admin/settings/share"],
  ["storage.", "/admin/settings/storage"],
  ["mail.", "/admin/settings/mail"],
  ["scan.", "/admin/settings/scan"],
  ["features.accounts", "/admin/settings/features"],
  ["features.admin", "/admin/settings/features"],
];

export function settingsSectionOf(key: string): string | null {
  return RULES.find(([match]) => key.startsWith(match))?.[1] ?? null;
}

/**
 * Keys whose fallback, when it is `false`, would switch off something the
 * admin needs to undo the reset: the panel itself, or signing in at all. A
 * reset leaves those alone, and the page says why.
 */
export const LOCKOUT_KEYS: ReadonlySet<string> = new Set([
  "features.admin",
  "features.accounts",
]);
