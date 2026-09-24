import { IconInfoCircle } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";

import { readSetting, useSettings } from "@/hooks/use-settings";

/**
 * 第三方登入遇到「這個人這裡還沒有帳號」時會怎麼做。
 *
 * 決定權在 `auth.registration` —— 同一個設定同時管密碼註冊跟第三方登入建帳號
 * （見 `api/oidc.rs` 的 `register`）。但它在另一頁，而且那頁的說明只講密碼，
 * 所以在這裡設 provider 的人沒有任何線索知道「關了註冊之後，第三方登入只剩
 * 現有帳號能用」。這一段就是補那條線索。
 *
 * 值一律讀 server 上實際生效的設定，不是表單狀態 —— 它講的是這台 instance 現在
 * 的行為（跟 `GuestAnonymousWarning` 同一個理由）。
 */
export function ProviderRegistrationNote() {
  const { data } = useSettings();
  const registration = readSetting(data, "auth.registration");

  return (
    <div className="flex gap-3 rounded-md border bg-muted/40 p-3 text-sm">
      <IconInfoCircle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <div className="space-y-1">
        <p className="font-medium">{headline(registration)}</p>
        <p className="text-muted-foreground">
          {detail(registration)}在
          <Link
            className="mx-1 underline underline-offset-4"
            to="/settings/account"
          >
            帳號與註冊
          </Link>
          改。
        </p>
      </div>
    </div>
  );
}

/**
 * 認不得的值當成 `open` —— 那是 server 的預設，說錯話不如說最可能的那句。
 * 三個值以外的東西只會在 server 加了新模式、面板還沒跟上時出現。
 */
function headline(registration: unknown): string {
  switch (registration) {
    case "closed": {
      return "目前只有現有帳號能用第三方登入";
    }
    case "invite": {
      return "目前第三方登入要有邀請碼才能開新帳號";
    }
    default: {
      return "目前第三方登入會自動替新的人開帳號";
    }
  }
}

function detail(registration: unknown): string {
  switch (registration) {
    case "closed": {
      return "註冊是關的，所以這裡還沒有帳號的人登入會被拒絕，看到的是「本站不開放註冊」。已經綁定的人、或用同一個已驗證信箱對上既有帳號的人不受影響。";
    }
    case "invite": {
      return "沒帶邀請碼的新使用者會被拒絕。邀請碼要跟著登入連結一起帶（`/login?invite=代碼`），被 provider 認證過不等於被這裡接受。";
    }
    default: {
      return "任何能通過這家 provider 認證的人都會拿到一個新帳號。如果那是公開服務（例如 Google），等於對全世界開放註冊。";
    }
  }
}
