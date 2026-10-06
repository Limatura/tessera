/**
 * 把登入後要回去的位址過濾成只剩同源的。
 *
 * `?redirect=` 是網址上的東西，任何人都能塞。`//evil.com`、`https://evil.com`
 * 瀏覽器都會當成外站，登入成功後直接把人送出去 —— 這是 open redirect，釣魚頁最
 * 愛的起手式。
 *
 * 同源的絕對網址照樣放行，轉成路徑：使用者手動貼上的完整網址也該能用。
 *
 * @param target 網址上帶進來的值，什麼都可能。
 * @param origin 比對的基準，正常呼叫傳 `window.location.origin`。
 * @returns 可以安全交給 router 導航的同源路徑，不安全就是 `null`。
 */
export function safeRedirect(
  target: string | undefined,
  origin: string
): string | null {
  if (!target) {
    return null;
  }

  // 一律先解析成絕對位址再比對 origin，不要自己判斷開頭長怎樣。瀏覽器會把
  // 反斜線正規化成斜線，所以 `/\evil.com` 跟 `//evil.com` 一樣會跳到外站，
  // 但它通得過「開頭是單一斜線」這種字串檢查。交給 URL 解析就沒有這種縫。
  try {
    const url = new URL(target, origin);
    return url.origin === origin
      ? `${url.pathname}${url.search}${url.hash}`
      : null;
  } catch {
    return null;
  }
}
