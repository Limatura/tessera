import * as m from "@/paraglide/messages";

/**
 * `start` 跟 `link` 都回 303，而且會把人帶去 provider 的網域。fetch 跟得動
 * 重導，但跟到 provider 那裡就會撞上 CORS —— 這兩支一定要讓瀏覽器自己走。
 */
function go(path: string) {
  window.location.assign(path);
}

/** 開始一次登入。有邀請碼就帶著，後端在成功建立帳號時才花掉它。 */
export function startSignIn(provider: string, invite?: string) {
  const query = invite ? `?invite=${encodeURIComponent(invite)}` : "";
  go(`/api/v1/auth/${encodeURIComponent(provider)}/start${query}`);
}

/** 把一家 provider 綁到目前登入的帳號上。回來會落在 /user。 */
export function startLink(provider: string) {
  go(`/api/v1/auth/${encodeURIComponent(provider)}/link`);
}

/**
 * 後端在失敗時把人送回 `/login?oidc_error=代碼` 或 `/user?oidc_error=代碼`。
 * 參數名三家共用，沒有跟著 provider 改。
 */
export const OIDC_ERROR_PARAM = "oidc_error";

/**
 * 九個代碼。後端只給代碼，句子是前端的事。
 *
 * 沒對到的值（後端加了新的、我們還沒跟上）回 null，由呼叫端退回一句通用的
 * 失敗訊息 —— 顯示一個生代碼給使用者沒有任何意義。
 */
export function oidcErrorMessage(code: string): string | null {
  switch (code) {
    case "unavailable": {
      return m.oidc_error_unavailable();
    }
    case "stale": {
      return m.oidc_error_stale();
    }
    case "refused": {
      return m.oidc_error_refused();
    }
    case "no_address": {
      return m.oidc_error_no_address();
    }
    case "not_permitted": {
      return m.oidc_error_not_permitted();
    }
    case "not_invited": {
      return m.oidc_error_not_invited();
    }
    case "address_taken": {
      return m.oidc_error_address_taken();
    }
    case "disabled": {
      return m.oidc_error_disabled();
    }
    case "already_linked": {
      return m.oidc_error_already_linked();
    }
    default: {
      return null;
    }
  }
}
