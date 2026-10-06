/**
 * 預覽卡片編輯器的純邏輯:佔位符怎麼填、每種狀態的卡片拿得到哪些值。
 *
 * 這裡是 server 那邊的鏡像,改了要一起改:
 *   - `fill` / `escapeHtml` ↔ `apps/server/src/og/layout.rs` 的 `fill`、`og/mod.rs` 的 `escape`
 *   - `sampleValues` ↔ `og/mod.rs` 的 `placeholder_values`
 *   - `WORDS` ↔ `apps/server/src/api/preview/words.rs`
 *
 * 規則(哪些元素、屬性可以用)刻意**不**鏡像 —— 那份交給 `POST /admin/settings/preview`
 * 去問 server,答案才會是真正存檔時那份。
 */

/** 卡片的四種狀態,對應 server 的 `Card`。 */
export const CARD_STATES = ["open", "locked", "sealed", "gone"] as const;
export type CardState = (typeof CARD_STATES)[number];

/** `site.language` 的值。卡片上的字用站台語言,不是看的人的語言。 */
export type SiteLanguage = "en" | "zh-TW" | "ja";

export const PLACEHOLDERS = [
  "title",
  "label",
  "size",
  "code",
  "expires",
  "footer",
  "language",
] as const;

/** `og::layout::MAX_BYTES`。 */
export const LAYOUT_MAX_BYTES = 8 * 1024;

export const CARD_WIDTH = 1200;
export const CARD_HEIGHT = 630;

export const SAMPLE_CODE = "K3P9XQ2M";
export const SAMPLE_TITLE = "Q3-2026-finance-and-operations-review.pdf";
const SAMPLE_SIZE = "24.8 MiB";
const SAMPLE_DAYS_LEFT = 3;
const SAMPLE_RETRIEVALS_LEFT = 5;

interface Words {
  sharedFile: string;
  lockedTitle: string;
  sealedTitle: string;
  goneTitle: string;
  labelFile: string;
  labelLocked: string;
  labelSealed: string;
  labelGone: string;
  lockedFooter: string;
  goneFooter: string;
  expiresInDays: (days: number) => string;
  left: (count: number) => string;
}

const WORDS: Record<SiteLanguage, Words> = {
  en: {
    expiresInDays: (days) => `expires in ${days} days`,
    goneFooter: "It expired, or it has already been collected",
    goneTitle: "Share unavailable",
    labelFile: "SHARED FILE",
    labelGone: "NO LONGER AVAILABLE",
    labelLocked: "PASSWORD PROTECTED",
    labelSealed: "END-TO-END ENCRYPTED",
    left: (count) =>
      count === 1 ? "one retrieval left" : `${count} retrievals left`,
    lockedFooter: "Enter the password to see what this is",
    lockedTitle: "Password-protected share",
    sealedTitle: "End-to-end encrypted share",
    sharedFile: "Shared file",
  },
  ja: {
    expiresInDays: (days) => `${days} 日後に期限切れ`,
    goneFooter: "有効期限が切れたか、取得可能な回数に達しました",
    goneTitle: "この共有は失効しました",
    labelFile: "共有ファイル",
    labelGone: "失効済み",
    labelLocked: "パスワード保護",
    labelSealed: "エンドツーエンド暗号化",
    left: (count) => `残り ${count} 回取得可能`,
    lockedFooter: "パスワードを入力すると内容を確認できます",
    lockedTitle: "パスワードで保護された共有",
    sealedTitle: "エンドツーエンド暗号化された共有",
    sharedFile: "共有ファイル",
  },
  "zh-TW": {
    expiresInDays: (days) => `${days} 天後到期`,
    goneFooter: "已過期，或已達到可提取的次數上限",
    goneTitle: "此分享已失效",
    labelFile: "分享的檔案",
    labelGone: "已失效",
    labelLocked: "受密碼保護",
    labelSealed: "端對端加密",
    left: (count) => `剩餘 ${count} 次提取`,
    lockedFooter: "輸入密碼即可查看內容",
    lockedTitle: "受密碼保護的分享",
    sealedTitle: "端對端加密的分享",
    sharedFile: "分享的檔案",
  },
};

export function asSiteLanguage(value: unknown): SiteLanguage {
  return value === "zh-TW" || value === "ja" ? value : "en";
}

/**
 * 某個狀態的分享會填進哪些值。
 *
 * 範例是「一個有標題、三天後到期、還能取五次的檔案」。鎖定的分享沒有 `size`、
 * `expires`:不是版面擋掉的,是值本來就不存在 —— 跟 server 同一個道理。
 */
export function sampleValues(
  state: CardState,
  language: SiteLanguage,
  title: string
): Partial<Record<(typeof PLACEHOLDERS)[number], string>> {
  const said = WORDS[language];
  const named = title.trim();
  const expires = said.expiresInDays(SAMPLE_DAYS_LEFT);
  const footer = `${expires} · ${said.left(SAMPLE_RETRIEVALS_LEFT)}`;

  switch (state) {
    case "open": {
      return {
        code: SAMPLE_CODE,
        expires,
        footer,
        label: said.labelFile,
        size: SAMPLE_SIZE,
        title: named || said.sharedFile,
      };
    }
    // 端對端加密的標題是我們自己的句子,uploader 打的標題不會出現。
    case "sealed": {
      return {
        code: SAMPLE_CODE,
        footer,
        label: said.labelSealed,
        size: SAMPLE_SIZE,
        title: said.sealedTitle,
      };
    }
    case "locked": {
      return {
        code: SAMPLE_CODE,
        footer: said.lockedFooter,
        label: said.labelLocked,
        title: named || said.lockedTitle,
      };
    }
    case "gone": {
      return {
        code: SAMPLE_CODE,
        footer: said.goneFooter,
        label: said.labelGone,
        title: said.goneTitle,
      };
    }
    default: {
      throw new Error(`unknown card state: ${state satisfies never}`);
    }
  }
}

export function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/**
 * 把值填進版面。沒有值的佔位符變成空字串;沒收尾的 `{{` 原樣留著。
 */
export function fill(
  html: string,
  values: Partial<Record<string, string>>
): string {
  let out = "";
  let rest = html;
  for (;;) {
    const open = rest.indexOf("{{");
    if (open === -1) {
      break;
    }
    out += rest.slice(0, open);
    const after = rest.slice(open + 2);
    const close = after.indexOf("}}");
    if (close === -1) {
      out += "{{";
      rest = after;
      continue;
    }
    const value = values[after.slice(0, close).trim()];
    if (value !== undefined) {
      out += escapeHtml(value);
    }
    rest = after.slice(close + 2);
  }
  return out + rest;
}

/** server 量的是 UTF-8 位元組,不是字元數 —— 中文一個字算三個。 */
export function layoutBytes(html: string): number {
  return new TextEncoder().encode(html).length;
}

/**
 * 「從範本開始」塞進去的版面。
 *
 * 跟內建卡片長得像,但只用佔位符做得到的事:內建卡片的大小是一顆膠囊,
 * 這裡做不到「沒有值就不畫膠囊」,鎖定的卡片會多一顆空膠囊,所以改成跟 label
 * 排在同一行的文字。
 */
export const STARTER_LAYOUT = `<div style="width:1200px;height:630px;display:flex;flex-direction:column;justify-content:space-between;padding:64px;background:#0a0605;color:#ffffff;font-family:DejaVu Sans">
  <div style="display:flex;align-items:center;gap:16px">
    <div style="width:40px;height:24px;background:#d94f2b"></div>
    <div style="font-size:24px;font-weight:700">Tessera</div>
  </div>
  <div style="display:flex;flex-direction:column;gap:16px">
    <div style="display:flex;gap:20px;font-size:20px;font-weight:700;letter-spacing:2px">
      <span style="color:#d94f2b">{{label}}</span>
      <span style="color:#b4a9a5">{{size}}</span>
    </div>
    <div style="font-size:56px;font-weight:700">{{title}}</div>
  </div>
  <div style="display:flex;align-items:flex-end;justify-content:space-between">
    <div style="font-size:22px;color:#b4a9a5">{{footer}}</div>
    <div style="border:2px dashed #d94f2b;border-radius:12px;padding:20px 32px;font-size:36px;font-weight:700;letter-spacing:4px">{{code}}</div>
  </div>
</div>
`;
