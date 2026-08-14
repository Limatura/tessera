import { z } from "zod";

/** Mirrors `Username::parse`'s rule in `crates/tessera-core/src/username.rs`. */
export const usernameSchema = z
  .string()
  .min(3, "Username must be at least 3 characters")
  .max(32, "Username must be at most 32 characters")
  .regex(
    /^[a-zA-Z0-9][a-zA-Z0-9_-]*$/u,
    "Username may only contain letters, digits, _ and -, and must start with a letter or digit"
  );

/** Floor/ceiling from `PasswordPolicy`; the server enforces its own minimum. */
export const passwordSchema = z
  .string()
  .min(8, "Password must be at least 8 characters")
  .max(1024, "Password is too long");

export const loginSchema = z.object({
  identifier: z.string().min(1, "Enter your username or email"),
  password: z.string().min(1, "Enter your password"),
});

export type LoginFormValues = z.infer<typeof loginSchema>;

export const registerSchema = z.object({
  email: z.string().email("Enter a valid email address"),
  password: passwordSchema,
  username: usernameSchema,
});

export type RegisterFormValues = z.infer<typeof registerSchema>;
