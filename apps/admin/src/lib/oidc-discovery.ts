import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * 這次讀取的結論。
 *
 * 成功時帶回的是「要填進表單的值」，不是整份文件 —— 端點本身也只回這幾個欄位，
 * 其餘後端登入時自己會讀（見 `auth/oidc/discovery.rs`）。
 */
export type ReadResult =
  | {
      ok: true;
      /** 文件自稱的 issuer。這是權威來源，不是從網址推出來的。 */
      issuer: string;
      /** 與 `scopes_supported` 對齊後的建議 scopes；provider 沒宣告就是 null。 */
      scopes: string[] | null;
      /** provider 宣告支援的全部 scopes，用來在畫面上解釋為什麼加了或少了幾個。 */
      supportedScopes: string[] | null;
      backchannelLogout: boolean;
    }
  | { ok: false; reason: string };

/** 後端無論如何都會補上這兩個（見 config.rs 的 `effective_scopes`）。 */
const ALWAYS_REQUESTED = ["openid", "email"];

/**
 * 這個站用得到、但表單預設沒有的 scope。
 *
 * 「必要群組」跟「群組 claim」兩個欄位都靠它 —— provider 支援卻沒要，群組
 * claim 根本不會出現在 token 裡，設了必要群組就是誰都進不來。
 */
const USEFUL_IF_SUPPORTED = ["groups"];

/**
 * 把想要的 scopes 收斂成這個 provider 真的認得的那些。
 *
 * Synology 的 `scopes_supported` 是 `["email", "groups", "openid"]` —— 沒有
 * `profile`，但表單預設就有。要一個 provider 沒宣告的 scope，輕則被忽略，重則
 * 整個授權請求被退回，而那時看到的只會是一個 `refused`。
 *
 * `openid` 跟 `email` 一律留著：後端不管表單填什麼都會補上它們，這裡拿掉只會
 * 讓畫面跟實際送出去的不一致。缺了它們是 provider 的問題，交給呼叫端去講。
 *
 * 反過來也會加：provider 支援 `groups` 而表單沒要的話就補上。這不是多此一舉
 * —— 少了它，「必要群組」設下去會變成誰都進不來，而錯誤只會是一個
 * `not_permitted`，看不出是 scope 沒要到。
 */
export function reconcileScopes(
  wanted: string[],
  supported: string[]
): string[] {
  const kept = wanted.filter(
    (scope) => supported.includes(scope) || ALWAYS_REQUESTED.includes(scope)
  );
  for (const required of ALWAYS_REQUESTED) {
    if (!kept.includes(required)) {
      kept.push(required);
    }
  }
  for (const useful of USEFUL_IF_SUPPORTED) {
    if (supported.includes(useful) && !kept.includes(useful)) {
      kept.push(useful);
    }
  }
  return kept;
}

/**
 * 請伺服器讀一次 provider 的 discovery 文件。
 *
 * # 為什麼不是瀏覽器自己抓
 *
 * Discovery 規格對瀏覽器讀取只寫 SHOULD，不少 provider（Synology DSM 就是一個）
 * 沒開放跨來源讀取，那在瀏覽器裡是永遠測不到的 —— 而那些正是自架站台最常接的。
 * 伺服器沒有 origin，CORS 不適用；而且它抓得到就代表登入路徑也抓得到，這件事
 * 瀏覽器本來就證明不了。見 #303。
 *
 * issuer 或 `.well-known` 位址都可以送，後端會自己處理字尾。回來的 `issuer`
 * 是文件自稱的那一個，是權威來源。
 */
export async function readDiscovery(
  url: string,
  wantedScopes: string[]
): Promise<ReadResult> {
  const { data, error } = await api.POST(
    "/api/v1/admin/settings/oidc/discovery",
    { body: { url } }
  );

  if (error) {
    /*
     * 後端把 `DiscoveryError` 的訊息原樣放進 400 的 message，它已經說明了是
     * 連不上、還是文件不能用（例如 issuer 對不起來、不接受 S256）。
     */
    const body = error as Partial<ErrorBody>;
    return { ok: false, reason: body.message || "讀不到這個位址的設定文件。" };
  }

  /*
   * 欄位是必填的陣列，沒宣告的 provider 會拿到空陣列。空的要當成「沒宣告」而不是
   * 「一個都不支援」—— 後者會把使用者填的 scopes 洗成只剩必要的那兩個。
   */
  const supportedScopes =
    data.scopes_supported.length > 0 ? data.scopes_supported : null;

  return {
    backchannelLogout: data.backchannel_logout_supported,
    issuer: data.issuer,
    ok: true,
    // 沒宣告就不動使用者填的 —— 沒宣告不代表不支援。
    scopes: supportedScopes
      ? reconcileScopes(wantedScopes, supportedScopes)
      : null,
    supportedScopes,
  };
}
