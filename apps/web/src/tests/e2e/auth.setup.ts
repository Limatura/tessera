import { expect, test as setup } from "@playwright/test";

import { ADMIN_STORAGE_STATE } from "../../../playwright.config";
import { ADMIN_USERNAME, adminPassword } from "./admin";

// Sign in once through the API, not the form: the database belongs to the Rust
// server, so the only honest way to a session is the same endpoint the app uses.
// The request context starts from the config's storageState, so the saved file
// keeps the locale cookie next to the session cookie.
setup("sign in as the administrator", async ({ request }) => {
  const response = await request.post("/api/v1/auth/login", {
    data: { identifier: ADMIN_USERNAME, password: adminPassword() },
  });
  expect(response.ok(), await response.text()).toBe(true);
  expect(await response.json()).not.toMatchObject({ status: "mfa_required" });

  await request.storageState({ path: ADMIN_STORAGE_STATE });
});
