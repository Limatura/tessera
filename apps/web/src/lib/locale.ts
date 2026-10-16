import {
  getTextDirection,
  setLocale as paraglideSetLocale,
} from "@/paraglide/runtime.js";
import type { Locale } from "@/paraglide/runtime.js";

/*
 * The generated Paraglide runtime plus what this app needs on top: a locale
 * switch that doesn't reload the document, and a way for React to know when
 * it happened.
 *
 * Paraglide's own `setLocale` reloads the document by default whatever the
 * configured strategy: `reload` defaults to `true`, and only the `url` strategy
 * ever gives it a navigation target, so the fallback is a bare
 * `window.location.reload()`. Every locale's strings are already in the
 * bundle, so that reload re-downloads the app to show text it already has.
 *
 * `{ reload: false }` is Paraglide's escape hatch for this, with a trade: it
 * doesn't re-render anything or touch the document, so nothing here is
 * reactive unless something says so. That is what `subscribeToLocale` and the
 * notify loop in `setLocale` are for: pair `getLocale` with
 * `subscribeToLocale` in `useSyncExternalStore`.
 */

/** Each language's name in that language, so a reader can find theirs. */
export const LOCALE_LABELS: Record<Locale, string> = {
  en: "English",
  ja: "日本語",
  "zh-CN": "简体中文",
  "zh-TW": "繁體中文",
};

const HTML_LANG: Record<Locale, string> = {
  en: "en",
  ja: "ja",
  "zh-CN": "zh-CN",
  "zh-TW": "zh-TW",
};

const listeners = new Set<() => void>();

export { getLocale } from "@/paraglide/runtime.js";

export function applyDocumentLocale(locale: Locale) {
  document.documentElement.lang = HTML_LANG[locale];
  document.documentElement.dir = getTextDirection(locale);
}

export function subscribeToLocale(onChange: () => void) {
  listeners.add(onChange);
  return () => {
    listeners.delete(onChange);
  };
}

export function setLocale(locale: Locale) {
  paraglideSetLocale(locale, { reload: false });
  for (const listener of listeners) {
    listener();
  }
}
