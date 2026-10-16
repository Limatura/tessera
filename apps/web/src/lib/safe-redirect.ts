/**
 * Filters the post-sign-in destination down to same-origin paths. Anyone can
 * set `?redirect=`, and browsers treat `//evil.com` and `https://evil.com` as
 * other sites, so a successful sign-in would send the user away: an open
 * redirect, a favourite opening move for phishing.
 *
 * A same-origin absolute URL is allowed and turned into a path, so a full URL
 * the user pasted still works.
 *
 * @param target The value from the URL; could be anything.
 * @param origin What to compare against; normally `window.location.origin`.
 * @returns A same-origin path safe to hand to the router, or `null`.
 */
export function safeRedirect(
  target: string | undefined,
  origin: string
): string | null {
  if (!target) {
    return null;
  }

  // Always resolve to an absolute URL and compare origins instead of checking
  // the prefix: browsers normalise backslashes to slashes, so `/\evil.com`
  // leaves the site like `//evil.com` yet passes a "starts with a single
  // slash" check. URL parsing leaves no such gap.
  try {
    const url = new URL(target, origin);
    return url.origin === origin
      ? `${url.pathname}${url.search}${url.hash}`
      : null;
  } catch {
    return null;
  }
}
