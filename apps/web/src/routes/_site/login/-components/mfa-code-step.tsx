import type { components } from "@tessera/api/schema";
import { useLoginWithCode } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@tessera/ui/components/field";
import { Input } from "@tessera/ui/components/input";
import { useState } from "react";

import { apiErrorMessage } from "@/lib/api-error";
import * as m from "@/paraglide/messages";

type ErrorBody = components["schemas"]["ErrorBody"];

/**
 * The second half of a password sign-in. The ticket lasts five minutes and
 * works once; when it is gone (`mfa_challenge_unusable`) the only way on is the
 * password again, so the error offers that instead of another try.
 */
export function MfaCodeStep({
  ticket,
  onSignedIn,
  onRestart,
}: {
  ticket: string;
  onSignedIn: () => void;
  onRestart: () => void;
}) {
  const loginWithCode = useLoginWithCode();
  const [code, setCode] = useState("");
  const error = loginWithCode.error as Partial<ErrorBody> | null;
  const expired = error?.code === "mfa_challenge_unusable";

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    loginWithCode.mutate(
      { code: code.trim(), ticket },
      { onSuccess: onSignedIn }
    );
  };

  return (
    <form onSubmit={handleSubmit}>
      <FieldGroup>
        <Field data-invalid={Boolean(error)}>
          <FieldLabel htmlFor="mfa-code">{m.auth_mfa_code_label()}</FieldLabel>
          <Input
            autoComplete="one-time-code"
            // oxlint-disable-next-line jsx-a11y/no-autofocus -- the only input on this step
            autoFocus
            disabled={expired}
            id="mfa-code"
            onChange={(event) => setCode(event.target.value)}
            placeholder="123456"
            spellCheck={false}
            value={code}
          />
          <FieldDescription>{m.auth_mfa_code_hint()}</FieldDescription>
          {error ? (
            <FieldError>
              {apiErrorMessage(error, m.common_try_again())}
            </FieldError>
          ) : null}
        </Field>
        <Field>
          {expired ? (
            <Button className="w-full" onClick={onRestart} type="button">
              {m.auth_mfa_restart()}
            </Button>
          ) : (
            <Button
              className="w-full"
              disabled={loginWithCode.isPending || code.trim() === ""}
              type="submit"
            >
              {loginWithCode.isPending
                ? m.auth_signing_in()
                : m.auth_mfa_verify()}
            </Button>
          )}
          {expired ? null : (
            <Button
              className="w-full"
              onClick={onRestart}
              type="button"
              variant="ghost"
            >
              {m.auth_mfa_back()}
            </Button>
          )}
        </Field>
      </FieldGroup>
    </form>
  );
}
