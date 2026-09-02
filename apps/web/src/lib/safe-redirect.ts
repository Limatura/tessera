/**
 * 把登入後要回去的位址過濾成只剩同源的。
 *
 * `?redirect=` 是網址上的東西，任何人都能塞。`//evil.com`、`https://evil.com`
 * 瀏覽器都會當成外站，登入成功後直接把人送出去 —— 這是 open redirect，釣魚頁最
 * 愛的起手式。
 *
 * 同源的絕對網址要放行：apps/admin 是另一包 SPA，被反向代理放在同一個網域下，
 * 它送過來的就是完整 URL。
 */
export function safeRedirect(target: string | undefined): string | null {
  if (!target) {
    return null;
  }

  // `//evil.com` 是 protocol-relative，會被當成外站，但開頭確實是斜線。
  if (target.startsWith("//")) {
    return null;
  }

  if (target.startsWith("/")) {
    return target;
  }

  try {
    const url = new URL(target);
    return url.origin === window.location.origin
      ? `${url.pathname}${url.search}${url.hash}`
      : null;
  } catch {
    return null;
  }
}
