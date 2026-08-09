import type { LocaleDictionaryMap } from "@tessera/i18n";

export const adminDictionaries = {
  en: {
    common: {
      language: "Language",
      switchLanguage: "Switch language",
    },
    header: {
      home: "Home",
      users: "Users",
    },
    home: {
      apiStatus: "System Status",
      description: "The default locale is set to Traditional Chinese.",
      subtitle: "This admin interface uses the shared i18n package.",
      title: "Admin dashboard",
    },
  },
  ja: {
    common: {
      language: "言語",
      switchLanguage: "言語を切り替える",
    },
    header: {
      home: "ホーム",
      users: "ユーザー",
    },
    home: {
      apiStatus: "システムステータス",
      description: "デフォルトの言語は繁体字中国語に設定されています。",
      subtitle: "この管理画面は共有 i18n パッケージを使用しています。",
      title: "管理ダッシュボード",
    },
  },
  tw: {
    common: {
      language: "語言",
      switchLanguage: "切換語言",
    },
    header: {
      home: "首頁",
      users: "使用者",
    },
    home: {
      apiStatus: "系統狀態",
      description: "預設語系已設定為繁體中文。",
      subtitle: "這個管理介面使用共享 i18n 套件。",
      title: "管理後台",
    },
  },
} satisfies LocaleDictionaryMap;
