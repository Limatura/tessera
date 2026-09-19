/**
 * 把 `created_user_agent` 的原始字串猜成「Chrome / macOS」。
 *
 * 後端不解析 UA,而且 issue #90 講得很直接:**解錯比不解糟**,因為把別人的 session
 * 標成你熟悉的瀏覽器,會讓人放過一筆該撤銷的登入。所以這裡只認幾條沒有歧義的樣式,
 * 認不出來就回 `null`,由畫面退回顯示原始字串 —— 原始字串很醜,但它不會騙人。
 *
 * 之所以不裝 ua-parser-js 之類的套件:那些函式庫的長處是「盡量猜出一個答案」,
 * 在這一頁剛好是短處。
 */

export interface ParsedUserAgent {
  browser: string | null;
  os: string | null;
}

/**
 * 順序就是規則。UA 字串裡的假冒是歷史共業:Edge 同時自稱 Chrome 和 Safari、
 * Chrome 自稱 Safari,所以最具體的要先比,比中就停。
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
 * iOS/iPadOS 要排在 macOS 前面:iPad 的 UA 裡也有 "Mac OS X"。
 * Android 要排在 Linux 前面,理由一樣。
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
 * 兩邊都認不出來時回 `null`,呼叫端才好分辨「沒答案」跟「一半有答案」。
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

/** 給畫面用的一行:「Chrome · macOS」,只認出一半就只顯示那一半。 */
export function formatUserAgent(userAgent?: string | null): string | null {
  const parsed = parseUserAgent(userAgent);
  if (!parsed) {
    return null;
  }
  return [parsed.browser, parsed.os].filter(Boolean).join(" · ");
}
