import { z } from "zod";

import * as m from "@/paraglide/messages";

/**
 * Character classes, matching `PasswordPolicy`'s doc comment in
 * `crates/tessera-core/src/password.rs`. Unicode, not ASCII, so a password in
 * any script is judged the same way the server judges it.
 *
 * A space is not a symbol: counting it would let any passphrase satisfy the
 * rule, which is not what an operator who turned it on asked for.
 */
const UPPERCASE = /\p{Uppercase}/u;
const LOWERCASE = /\p{Lowercase}/u;
const DIGIT = /\p{N}/u;
const SYMBOL = /[^\p{Alphabetic}\p{N}\p{White_Space}]/u;

/** `PasswordPolicy::FLOOR` / `CEILING`. */
export const PASSWORD_MIN_FLOOR = 8;
export const PASSWORD_MAX_CEILING = 1024;

/** What the server will accept for a *newly chosen* password. */
export interface PasswordPolicy {
  minLength: number;
  requiresMixedCase: boolean;
  requiresDigit: boolean;
  requiresSymbol: boolean;
  /**
   * Whether a password in a known breach is refused, and so whether
   * `GET /auth/pwned/{prefix}` is there to ask while somebody types.
   */
  checksBreaches: boolean;
}

/** Used until `/capabilities` answers, and if it never does. */
export const DEFAULT_PASSWORD_POLICY: PasswordPolicy = {
  checksBreaches: false,
  minLength: PASSWORD_MIN_FLOOR,
  requiresDigit: false,
  requiresMixedCase: false,
  requiresSymbol: false,
};

/** One rule, and whether the password in hand satisfies it. */
export interface PasswordRule {
  id: "length" | "mixedCase" | "digit" | "symbol";
  label: string;
  satisfied: boolean;
}

/**
 * The rules `policy` turns on, checked against `value`.
 *
 * Length is always a rule; the composition ones appear only when the operator
 * asked for them, so an instance on the defaults shows a single line.
 */
export function passwordRules(
  policy: PasswordPolicy,
  value: string
): PasswordRule[] {
  const rules: PasswordRule[] = [
    {
      id: "length",
      label: m.auth_password_rule_length({ min: policy.minLength }),
      // The server counts characters, not UTF-16 code units.
      satisfied: [...value].length >= policy.minLength,
    },
  ];

  if (policy.requiresMixedCase) {
    rules.push({
      id: "mixedCase",
      label: m.auth_password_rule_mixed_case(),
      satisfied: UPPERCASE.test(value) && LOWERCASE.test(value),
    });
  }
  if (policy.requiresDigit) {
    rules.push({
      id: "digit",
      label: m.auth_password_rule_digit(),
      satisfied: DIGIT.test(value),
    });
  }
  if (policy.requiresSymbol) {
    rules.push({
      id: "symbol",
      label: m.auth_password_rule_symbol(),
      satisfied: SYMBOL.test(value),
    });
  }

  return rules;
}

/**
 * A zod schema for choosing a new password under `policy`.
 *
 * Every unmet rule is reported, not just the first, so someone who is missing
 * both a digit and a symbol learns that in one attempt instead of two.
 */
export function passwordSchemaFor(policy: PasswordPolicy) {
  return z
    .string()
    .max(PASSWORD_MAX_CEILING, { error: () => m.auth_password_too_long() })
    .superRefine((value, ctx) => {
      for (const rule of passwordRules(policy, value)) {
        if (!rule.satisfied) {
          ctx.addIssue({ code: "custom", message: rule.label });
        }
      }
    });
}

/**
 * Whether the server turned the password down because it appears in a known
 * breach (`auth.pwned.*`). That refusal is a plain 400 whose message names the
 * breach, with no code of its own, so the message is all there is to go on.
 *
 * It belongs on the password field, next to the other reasons a password is
 * refused, rather than under the submit button like a failure of the request.
 */
export function isBreachedPasswordError(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string" &&
    error.message.includes("data breach")
  );
}
