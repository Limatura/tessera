import { z } from "zod";

import * as m from "@/paraglide/messages";

const USERNAME_MIN_LENGTH = 3;
const USERNAME_MAX_LENGTH = 32;
const PASSWORD_MIN_LENGTH = 8;
const PASSWORD_MAX_LENGTH = 1024;

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

/** Floor/ceiling from `PasswordPolicy`; the server enforces its own minimum. */
export const passwordSchema = z
  .string()
  .min(PASSWORD_MIN_LENGTH, {
    error: () => m.auth_password_too_short({ min: PASSWORD_MIN_LENGTH }),
  })
  .max(PASSWORD_MAX_LENGTH, { error: () => m.auth_password_too_long() });

export const loginSchema = z.object({
  identifier: z.string().min(1, { error: () => m.auth_identifier_required() }),
  password: z.string().min(1, { error: () => m.auth_password_required() }),
});

export type LoginFormValues = z.infer<typeof loginSchema>;

export const registerSchema = z.object({
  email: z.email({ error: () => m.auth_email_invalid() }),
  password: passwordSchema,
  username: usernameSchema,
});

export type RegisterFormValues = z.infer<typeof registerSchema>;
