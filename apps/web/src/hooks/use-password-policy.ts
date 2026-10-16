import { useMemo } from "react";

import { DEFAULT_PASSWORD_POLICY } from "@/lib/api/password-policy";
import type { PasswordPolicy } from "@/lib/api/password-policy";

import { useCapabilities } from "./use-capabilities";

/**
 * The instance's rules for choosing a new password.
 *
 * Falls back to the defaults while `/capabilities` is in flight, so a form
 * renders immediately. The server is the authority either way: checking here
 * only saves a round trip and names which rule is missing.
 */
export function usePasswordPolicy(): PasswordPolicy {
  const { data } = useCapabilities();

  // Stable across renders, so a schema derived from it is not rebuilt on every
  // keystroke.
  return useMemo(() => {
    if (!data) {
      return DEFAULT_PASSWORD_POLICY;
    }

    return {
      checksBreaches: data.account_password_breach_check,
      minLength: data.account_password_min_length,
      requiresDigit: data.account_password_requires_digit,
      requiresMixedCase: data.account_password_requires_mixed_case,
      requiresSymbol: data.account_password_requires_symbol,
    };
  }, [data]);
}
