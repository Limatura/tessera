/**
 * Guesses "Chrome / macOS" from the raw `created_user_agent`. The server does
 * not parse user agents, and a wrong answer is worse than none: labelling
 * someone else's session as your familiar browser lets a sign-in that should
 * be revoked slip past. So only a few unambiguous patterns are recognised, and
 * anything else returns `null` so the screen shows the raw string, ugly but
 * honest. No ua-parser-js or similar: those libraries are good at always
 * producing a guess, which on this page is the weakness.
 */

export interface ParsedUserAgent {
  browser: string | null;
  os: string | null;
}

/**
 * Order is the rule: Edge also claims Chrome and Safari, and Chrome claims Safari, so the
 * most specific goes first and the first match wins.
 */
const BROWSERS: [name: string, pattern: RegExp][] = [
  ["Edge", /\bEdg(?:e|A|iOS)?\//u],
  ["Opera", /\bOPR\/|\bOpera\//u],
  ["Samsung Internet", /\bSamsungBrowser\//u],
  ["Firefox", /\bFirefox\/|\bFxiOS\//u],
  ["Chrome", /\bChrome\/|\bCriOS\//u],
  ["Safari", /\bVersion\/[\d.]+ (?:Mobile\/\S+ )?Safari\//u],
  ["curl", /^curl\//u],
  ["Tessera CLI", /^tessera\//iu],
];

/**
 * iOS/iPadOS before macOS, since an iPad's UA also contains "Mac OS X". Android before
 * Linux for the same reason.
 */
const OPERATING_SYSTEMS: [name: string, pattern: RegExp][] = [
  ["iOS", /\((?:iPhone|iPad|iPod)\b/u],
  ["Android", /\bAndroid\b/u],
  ["Windows", /\bWindows NT\b/u],
  ["macOS", /\bMac OS X\b|\bMacintosh\b/u],
  ["Linux", /\bLinux\b|\bX11\b/u],
];

function matchFirst(
  table: [name: string, pattern: RegExp][],
  userAgent: string
): string | null {
  for (const [name, pattern] of table) {
    if (pattern.test(userAgent)) {
      return name;
    }
  }
  return null;
}

/**
 * `null` when neither part is recognised, so callers can tell "no answer" from "half an
 * answer".
 */
export function parseUserAgent(
  userAgent?: string | null
): ParsedUserAgent | null {
  if (!userAgent) {
    return null;
  }

  const browser = matchFirst(BROWSERS, userAgent);
  const os = matchFirst(OPERATING_SYSTEMS, userAgent);

  if (!(browser || os)) {
    return null;
  }

  return { browser, os };
}

/**
 * One line for the screen, "Chrome · macOS", showing only the half that was recognised.
 */
export function formatUserAgent(userAgent?: string | null): string | null {
  const parsed = parseUserAgent(userAgent);
  if (!parsed) {
    return null;
  }
  return [parsed.browser, parsed.os].filter(Boolean).join(" · ");
}
