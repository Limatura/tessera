import { z } from "zod";

import * as m from "@/paraglide/messages";

import { passwordSchemaFor } from "./password-policy";
import type { PasswordPolicy } from "./password-policy";

const USERNAME_MIN_LENGTH = 3;
const USERNAME_MAX_LENGTH = 32;

/** Mirrors `Username::parse`'s rule in `crates/tessera-core/src/username.rs`. */
export const usernameSchema = z
  .string()
  .min(USERNAME_MIN_LENGTH, {
    error: () => m.auth_username_too_short({ min: USERNAME_MIN_LENGTH }),
  })
  .max(USERNAME_MAX_LENGTH, {
    error: () => m.auth_username_too_long({ max: USERNAME_MAX_LENGTH }),
  })
  .regex(/^[a-zA-Z0-9][a-zA-Z0-9_-]*$/u, {
    error: () => m.auth_username_invalid_chars(),
  });

export const loginSchema = z.object({
  identifier: z.string().min(1, { error: () => m.auth_identifier_required() }),
  // Not the policy: an account created under an older, narrower one must still
  // be able to sign in. The server parses sign-ins against the widest bounds
  // for the same reason.
  password: z.string().min(1, { error: () => m.auth_password_required() }),
});

export type LoginFormValues = z.infer<typeof loginSchema>;

/** Registration checks the password the instance would actually accept. */
export function registerSchemaFor(policy: PasswordPolicy) {
  return z.object({
    email: z.email({ error: () => m.auth_email_invalid() }),
    password: passwordSchemaFor(policy),
    username: usernameSchema,
  });
}

export interface RegisterFormValues {
  email: string;
  password: string;
  username: string;
}
