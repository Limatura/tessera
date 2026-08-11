export type TextDirection = "ltr" | "rtl";

/**
 * The bits of a generated Paraglide `runtime.js` this package needs. Each app
 * compiles its own runtime (own `Locale` union, own `outdir`), so this is
 * injected rather than imported directly.
 */
export interface LocaleRuntime<Locale extends string> {
  getLocale: () => Locale;
  setLocale: (locale: Locale, options?: { reload?: boolean }) => void;
  getTextDirection: (locale: Locale) => TextDirection;
}

/**
 * Wraps a generated Paraglide runtime with the bits every app here needs on
 * top of it: a locale switch that doesn't reload the document, and a way for
 * React to know when it happened.
 *
 * Paraglide's own `setLocale` reloads the document by default no matter the
 * configured strategy — `reload` defaults to `true`, and nothing short of the
 * `url` strategy ever gives it a navigation target, so the fallback is a bare
 * `window.location.reload()`. Every locale's strings are already in the
 * bundle, so that reload just re-downloads the app to show text it already
 * has.
 *
 * `{ reload: false }` is the escape hatch Paraglide ships for exactly this,
 * but it comes with a trade: it doesn't re-render anything or touch the
 * document itself, so nothing here is reactive unless something says so.
 * That's what `subscribeToLocale` / the notify loop below are for — pair
 * `getLocale` with `subscribeToLocale` in `useSyncExternalStore` and it is.
 *
 * `htmlLang` and its `Locale` type are per-project (tied to that app's
 * `project.inlang`), so they stay app-side and are passed in rather than
 * hardcoded here.
 */
export function createLocaleControls<Locale extends string>(
  runtime: LocaleRuntime<Locale>,
  htmlLang: Record<Locale, string>
) {
  const listeners = new Set<() => void>();

  const applyDocumentLocale = (locale: Locale) => {
    document.documentElement.lang = htmlLang[locale];
    document.documentElement.dir = runtime.getTextDirection(locale);
  };

  const subscribeToLocale = (onChange: () => void) => {
    listeners.add(onChange);
    return () => {
      listeners.delete(onChange);
    };
  };

  const setLocale = (locale: Locale) => {
    runtime.setLocale(locale, { reload: false });
    for (const listener of listeners) {
      listener();
    }
  };

  return {
    applyDocumentLocale,
    getLocale: runtime.getLocale,
    setLocale,
    subscribeToLocale,
  };
}
