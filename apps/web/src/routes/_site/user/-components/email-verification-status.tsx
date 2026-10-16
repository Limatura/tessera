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
    // Already verified elsewhere. The session is refetched, and this status
    // switches to "verified" by itself.
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
    // 503: the relay refused the mail; 404: no mail set up here (only seen with
    // stale capabilities).
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
 * The verification status of the current email, with a resend while unverified.
 *
 * Whether mail can be sent comes from `capabilities.password_reset`: both need mail
 * configured on this instance. Without it only the status shows, not a button that is
 * certain to fail.
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
      <Badge
        variant="secondary"
        className="bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300"
      >
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
    <div className="flex flex-wrap items-center gap-2 text-sm">
      {capabilities?.password_reset && !request.isSuccess ? (
        <Button
          disabled={request.isPending}
          onClick={handleResend}
          size="sm"
          type="button"
          variant="link"
        >
          <IconMail />
          {m.user_email_verification_send()}
        </Button>
      ) : null}
      <Badge variant="destructive">
        <IconAlertCircle />
        {m.user_email_unverified()}
      </Badge>
      {request.isSuccess ? (
        <span className="text-muted-foreground">
          {m.user_email_verification_sent()}
        </span>
      ) : null}
    </div>
  );
}
