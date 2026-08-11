import { createLocaleControls } from "@tessera/i18n";

import {
  getLocale as paraglideGetLocale,
  getTextDirection,
  setLocale as paraglideSetLocale,
} from "@/paraglide/runtime.js";
import type { Locale } from "@/paraglide/runtime.js";

// 文件名用 tw，但 HTML lang 属性需要标准语言标签
const HTML_LANG: Record<Locale, string> = {
  en: "en",
  ja: "ja",
  tw: "zh-Hant",
};

export const { applyDocumentLocale, getLocale, setLocale, subscribeToLocale } =
  createLocaleControls<Locale>(
    {
      getLocale: paraglideGetLocale,
      getTextDirection,
      setLocale: paraglideSetLocale,
    },
    HTML_LANG
  );
