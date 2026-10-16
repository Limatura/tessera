import { IconFingerprint } from "@tabler/icons-react";
import { useMutation } from "@tanstack/react-query";
import { usePasskeySignIn } from "@tessera/auth/hooks";
import { Button } from "@tessera/ui/components/button";
import { FieldError } from "@tessera/ui/components/field";

import { apiErrorMessage } from "@/lib/api-error";
import {
  isPasskeyCancelled,
  isPasskeySupported,
  requestPasskey,
} from "@/lib/webauthn";
import * as m from "@/paraglide/messages";

/**
 * "Sign in with a passkey". Hidden where the browser cannot do it: a button
 * that can only fail is worse than none, and the password form is right there.
 */
export function PasskeySignInButton({
  onSignedIn,
}: {
  onSignedIn: () => void;
}) {
  // The browser prompt and the server call are separate so a cancelled prompt
  // never reaches the server, and shows nothing.
  const prompt = useMutation({ mutationFn: requestPasskey });
  const signIn = usePasskeySignIn();

  if (!isPasskeySupported()) {
    return null;
  }

  const handleClick = () => {
    signIn.reset();
    prompt.mutate(undefined, {
      onSuccess: (credential) =>
        signIn.mutate(credential, { onSuccess: onSignedIn }),
    });
  };

  const error =
    signIn.error ??
    (prompt.error && !isPasskeyCancelled(prompt.error) ? prompt.error : null);

  return (
    <>
      <Button
        className="w-full"
        disabled={prompt.isPending || signIn.isPending}
        onClick={handleClick}
        type="button"
        variant="outline"
      >
        <IconFingerprint size={16} />
        {m.auth_passkey_sign_in()}
      </Button>
      {error ? (
        <FieldError>{apiErrorMessage(error, m.common_try_again())}</FieldError>
      ) : null}
    </>
  );
}
