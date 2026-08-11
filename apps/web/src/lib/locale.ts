import {
  deLocalizeUrl,
  getTextDirection,
  localizeUrl,
  overwriteSetLocale,
} from "@/paraglide/runtime.js";
import type { Locale } from "@/paraglide/runtime.js";

/**
 * Re-exported so that everything about the locale arrives from one module, and
 * nothing outside it has to know that `setLocale` has been replaced below.
 */
export { getLocale } from "@/paraglide/runtime.js";

/**
 * What `<html lang>` should say for each of our locales.
 *
 * The message files are named after the audience (`tw`), which is not a
 * language tag — `tw` is Twi. Screen readers and translation prompts read the
 * attribute, so the mapping happens here rather than renaming every file.
 */
const HTML_LANG: Record<Locale, string> = {
  en: "en",
  ja: "ja",
  tw: "zh-Hant",
};

/**
 * Moves the locale between the address bar and the router.
 *
 * Paraglide owns the locale prefix (`/en/s/ABC`, and no prefix at all for the
 * base locale). The router never sees it: `input` strips it before a route is
 * matched, `output` puts the current one back before the URL is committed. So
 * the route tree stays `/` and `/s/$code` with no `$locale` segment, and every
 * `<Link>` gets the right prefix without knowing that locales exist.
 */
export const localeRewrite = {
  input: ({ url }: { url: URL }) => deLocalizeUrl(url),
  output: ({ url }: { url: URL }) => localizeUrl(url),
};

const listeners = new Set<() => void>();

const notify = () => {
  for (const listener of listeners) {
    listener();
  }
};

/**
 * Switch locales without reloading the document.
 *
 * Paraglide's own `setLocale` assigns to `window.location` under the `url`
 * strategy — a full page load, which its source calls acceptable because
 * "a user rarely switches locales" and points at this hook for when it is not.
 * It is not, here: every locale's strings are already in the bundle, so a
 * reload re-downloads the app to show text the browser is holding.
 *
 * `getLocale()` re-reads `window.location` on every call and does not cache, so
 * rewriting the address bar is the whole of the state change. The `notify()`
 * is only to tell React that something it cannot observe has moved.
 */
overwriteSetLocale((newLocale) => {
  const target = localizeUrl(window.location.href, { locale: newLocale });
  // Also the guard for `getLocale()`'s first call, which re-asserts the locale
  // it just resolved from this same URL.
  if (target.href === window.location.href) {
    return;
  }
  window.history.pushState(
    null,
    "",
    target.pathname + target.search + target.hash
  );
  notify();
});

export const subscribeToLocale = (onChange: () => void) => {
  listeners.add(onChange);
  // Back and forward move the prefix too, and the router has nothing to say
  // about it: both URLs strip to the same route.
  window.addEventListener("popstate", onChange);
  return () => {
    listeners.delete(onChange);
    window.removeEventListener("popstate", onChange);
  };
};

/** Keeps `<html lang>` and `dir` in step with the locale in the URL. */
export const applyDocumentLocale = (locale: Locale) => {
  document.documentElement.lang = HTML_LANG[locale];
  document.documentElement.dir = getTextDirection(locale);
};
