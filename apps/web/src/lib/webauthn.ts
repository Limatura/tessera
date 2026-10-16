import {
  startAuthentication,
  startRegistration,
} from "@simplewebauthn/browser";
import type {
  PublicKeyCredentialCreationOptionsJSON,
  PublicKeyCredentialRequestOptionsJSON,
} from "@simplewebauthn/browser";
import { api } from "@tessera/api";
import type { components } from "@tessera/api/schema";

type RegistrationCredential = components["schemas"]["RegistrationCredential"];
type AuthenticationCredential =
  components["schemas"]["AuthenticationCredential"];

export { browserSupportsWebAuthn as isPasskeySupported } from "@simplewebauthn/browser";

/**
 * The user closed the browser's prompt, or it timed out. Not worth an error
 * message: they know what they did. SimpleWebAuthn keeps the DOMException's
 * name on the error it rethrows.
 */
export function isPasskeyCancelled(error: unknown): boolean {
  return error instanceof Error && error.name === "NotAllowedError";
}

/**
 * Asks the browser for a passkey to sign in with. Nobody is named: the browser
 * offers whichever ones it holds for this site.
 */
export async function requestPasskey(): Promise<AuthenticationCredential> {
  const { data, error } = await api.POST("/api/v1/auth/passkey/options");
  if (error) {
    throw error;
  }
  const credential = await startAuthentication({
    optionsJSON: data as unknown as PublicKeyCredentialRequestOptionsJSON,
  });
  return credential as unknown as AuthenticationCredential;
}

/**
 * Has the browser make a new passkey for the signed-in account. The options
 * call is where the owner is confirmed, so it takes the current password.
 */
export async function createPasskey(
  currentPassword: string | undefined
): Promise<RegistrationCredential> {
  const { data, error } = await api.POST("/api/v1/account/passkeys/options", {
    body: { current_password: currentPassword },
  });
  if (error) {
    throw error;
  }
  const credential = await startRegistration({
    optionsJSON: data as unknown as PublicKeyCredentialCreationOptionsJSON,
  });
  return credential as unknown as RegistrationCredential;
}
