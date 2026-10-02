import {
  IconAlertCircle,
  IconCircleCheck,
  IconMail,
} from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import { toast } from "@tessera/ui/components/toast";

import type { VerificationRequestError } from "@/hooks/use-account";
import { useRequestEmailVerification } from "@/hooks/use-account";
import { useCapabilities } from "@/hooks/use-capabilities";
import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

const SECONDS_PER_MINUTE = 60;

function errorDescription(error: VerificationRequestError): string {
  switch (error.code) {
    // 別處已經驗證過了。session 會被重抓,這個狀態列自己會換成「已驗證」。
    case "email_already_verified": {
      return m.user_email_verification_already();
    }
    case "too_many_requests": {
      const minutes = Math.max(
        1,
        Math.ceil(
          (error.retryAfterSeconds ?? SECONDS_PER_MINUTE) / SECONDS_PER_MINUTE
        )
      );
      return m.user_email_verification_throttled({ minutes });
    }
    // 503 是 relay 不收信;404 是這台沒設寄信(capabilities 過期時才會碰到)。
    case "unavailable":
    case "not_found": {
      return m.user_email_verification_unavailable();
    }
    default: {
      return apiErrorMessage(error, m.common_try_again());
    }
  }
}

/**
 * 目前 email 的驗證狀態,未驗證時附上重寄驗證信。
 *
 * 能不能寄信看 `capabilities.password_reset` —— 兩者的條件一樣,都是這台有設寄信。
 * 寄不了信的站台只顯示狀態,不給一個按了必定失敗的按鈕。
 */
export function EmailVerificationStatus({
  verifiedAt,
}: {
  verifiedAt: string | null;
}) {
  const { data: capabilities } = useCapabilities();
  const request = useRequestEmailVerification();

  if (verifiedAt) {
    return (
      <Badge variant="secondary">
        <IconCircleCheck />
        {m.user_email_verified()}
      </Badge>
    );
  }

  const handleResend = () => {
    request.mutate(undefined, {
      onError: (error) =>
        toast.add({
          description: errorDescription(error),
          title: m.user_email_verification_failed(),
          type: error.code === "email_already_verified" ? "info" : "error",
        }),
    });
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Badge variant="outline">
        <IconAlertCircle />
        {m.user_email_unverified()}
      </Badge>
      {request.isSuccess ? (
        <span className="text-muted-foreground text-xs">
          {m.user_email_verification_sent()}
        </span>
      ) : null}
      {capabilities?.password_reset && !request.isSuccess ? (
        <Button
          disabled={request.isPending}
          onClick={handleResend}
          size="xs"
          type="button"
          variant="link"
        >
          <IconMail />
          {m.user_email_verification_send()}
        </Button>
      ) : null}
    </div>
  );
}
