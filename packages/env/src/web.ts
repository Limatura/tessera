import { createEnv } from "@t3-oss/env-core";

/**
 * Frontend environment variables.
 *
 * Only `VITE_`-prefixed ones are readable.
 */
export const env = createEnv({
  // None at the moment: since the admin panel moved into apps/web, the
  // cross-links (`VITE_FRONTEND_URL`, `VITE_ADMIN_URL`) are routes in one router.
  client: {},
  clientPrefix: "VITE_",
  emptyStringAsUndefined: true,
  runtimeEnv: import.meta.env,
  skipValidation: !!process.env.SKIP_ENV_VALIDATION,
});
