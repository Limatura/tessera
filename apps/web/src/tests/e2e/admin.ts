/**
 * The administrator the e2e backend bootstraps. Start the server with
 * `TESSERA_AUTH__ADMIN_PASSWORD` and pass the same value here as
 * `E2E_ADMIN_PASSWORD`; the username defaults to the server's `admin`.
 */
export const ADMIN_USERNAME = process.env.E2E_ADMIN_USERNAME ?? "admin";

export function adminPassword(): string {
  const password = process.env.E2E_ADMIN_PASSWORD;
  if (!password) {
    throw new Error(
      "Set E2E_ADMIN_PASSWORD to the backend's TESSERA_AUTH__ADMIN_PASSWORD (see src/tests/e2e/README.md)."
    );
  }
  return password;
}
