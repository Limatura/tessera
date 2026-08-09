import { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

export type Locale = "en" | "ja" | "tw";
export type LocaleDictionaryMap = Record<Locale, Record<string, unknown>>;

const STORAGE_KEY = "tessera-locale";
const DEFAULT_LOCALE: Locale = "tw";
const AVAILABLE_LOCALES: Locale[] = ["en", "ja", "tw"];

function resolveLocale(value?: string | null): Locale {
  if (value === "ja" || value === "jp") {
    return "ja";
  }

  if (
    value === "tw" ||
    value === "zh" ||
    value === "zh-TW" ||
    value === "zh-tw"
  ) {
    return "tw";
  }

  if (value === "en") {
    return "en";
  }

  return DEFAULT_LOCALE;
}

function getValue(
  dictionary: Record<string, unknown>,
  key: string
): string | undefined {
  const segments = key.split(".");
  let current: unknown = dictionary;

  for (const segment of segments) {
    if (
      typeof current !== "object" ||
      current === null ||
      !(segment in current)
    ) {
      return undefined;
    }

    current = (current as Record<string, unknown>)[segment];
  }

  return typeof current === "string" ? current : undefined;
}

export function createTranslator(dictionaries: LocaleDictionaryMap) {
  return function translate(locale: Locale, key: string): string {
    const fallbackValue = getValue(dictionaries.en, key);
    const value = getValue(dictionaries[locale], key) ?? fallbackValue;

    return value ?? key;
  };
}

export interface I18nContextValue {
  locale: Locale;
  setLocale: (nextLocale: Locale) => void;
  t: (key: string) => string;
  availableLocales: Locale[];
}

const I18nContext = createContext<I18nContextValue | null>(null);

interface I18nProviderProps {
  children: ReactNode;
  defaultLocale?: Locale;
  dictionaries: LocaleDictionaryMap;
}

export function I18nProvider({
  children,
  defaultLocale = DEFAULT_LOCALE,
  dictionaries,
}: I18nProviderProps) {
  const [locale, setLocaleState] = useState<Locale>(() => {
    if (typeof window === "undefined") {
      return defaultLocale;
    }

    const stored = window.localStorage.getItem(STORAGE_KEY);
    return resolveLocale(stored ?? window.navigator.language) ?? defaultLocale;
  });

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    window.localStorage.setItem(STORAGE_KEY, locale);
    document.documentElement.lang =
      locale === "ja" ? "ja" : (locale === "tw" ? "zh-TW" : "en");
  }, [locale]);

  const value = useMemo<I18nContextValue>(() => {
    const translate = createTranslator(dictionaries);

    return {
      availableLocales: AVAILABLE_LOCALES,
      locale,
      setLocale: (nextLocale: Locale) => setLocaleState(nextLocale),
      t: (key: string) => translate(locale, key),
    };
  }, [dictionaries, locale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const context = useContext(I18nContext);

  if (!context) {
    throw new Error("useI18n must be used within an I18nProvider");
  }

  return context;
}
