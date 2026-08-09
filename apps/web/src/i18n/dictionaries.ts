import type { LocaleDictionaryMap } from "@tessera/i18n";

export const webDictionaries = {
  en: {
    common: {
      language: "Language",
      switchLanguage: "Switch language",
    },
    header: {
      dashboard: "Dashboard",
      home: "Home",
    },
    home: {
      apiStatus: "API Status",
      description: "The default locale is set to Traditional Chinese.",
      subtitle: "This app uses the shared monorepo i18n package.",
      title: "Welcome to the Web app",
    },
  },
  ja: {
    common: {
      language: "言語",
      switchLanguage: "言語を切り替える",
    },
    header: {
      dashboard: "ダッシュボード",
      home: "ホーム",
    },
    home: {
      apiStatus: "API ステータス",
      description: "デフォルトの言語は繁体字中国語に設定されています。",
      subtitle: "このアプリは共有モノレポの i18n パッケージを使用しています。",
      title: "Web アプリへようこそ",
    },
  },
  tw: {
    common: {
      language: "語言",
      switchLanguage: "切換語言",
    },
    header: {
      dashboard: "控制台",
      home: "首頁",
    },
    home: {
      apiStatus: "API 狀態",
      description: "預設語系已設定為繁體中文。",
      subtitle: "這個應用使用共享 monorepo 的 i18n 套件。",
      title: "歡迎來到 Web 應用",
    },
  },
} satisfies LocaleDictionaryMap;
