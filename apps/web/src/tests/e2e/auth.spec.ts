import { expect, test } from "@playwright/test";

import { ADMIN_USERNAME, adminPassword } from "./admin";

// Tests tagged @authenticated run in the `authenticated` project with the
// session auth.setup.ts saved; the rest run in `anonymous` with no session.

test.describe("signed out", () => {
  test("the admin console sends a visitor to sign in", async ({ page }) => {
    await page.goto("/admin");
    await expect(page).toHaveURL(/\/login\?redirect=/u);
    await expect(page.getByText("Welcome back")).toBeVisible();
  });

  test("a wrong password is refused", async ({ page }) => {
    await page.goto("/login");
    await page.getByLabel("Username or email").fill(ADMIN_USERNAME);
    await page.getByLabel("Password", { exact: true }).fill("not-the-password");
    await page.getByRole("button", { exact: true, name: "Sign in" }).click();

    await expect(page.getByText("Wrong username or password.")).toBeVisible();
    await expect(page).toHaveURL(/\/login/u);
  });

  test("signing in through the form returns to the page that asked, and logout ends the session", async ({
    page,
  }) => {
    await page.goto("/admin");
    await expect(page).toHaveURL(/\/login\?redirect=/u);

    await page.getByLabel("Username or email").fill(ADMIN_USERNAME);
    await page.getByLabel("Password", { exact: true }).fill(adminPassword());
    await page.getByRole("button", { exact: true, name: "Sign in" }).click();

    await expect(page).toHaveURL(/\/admin\/dashboard/u);

    // A session of its own, so logging out leaves the shared one alone.
    await page.goto("/");
    await page.getByRole("button", { name: /^[A-Z]{1,2}$/u }).click();
    await page.getByRole("menuitem", { name: "Logout" }).click();
    await expect(page.locator('nav a[href="/login"]')).toBeVisible();

    await page.goto("/admin");
    await expect(page).toHaveURL(/\/login\?redirect=/u);
  });
});

test.describe("signed in", { tag: "@authenticated" }, () => {
  test("the admin console opens on the dashboard", async ({ page }) => {
    await page.goto("/admin");
    await expect(page).toHaveURL(/\/admin\/dashboard/u);
    await expect(page.getByText("Dashboard").first()).toBeVisible();
  });

  test("the user menu offers the admin console", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: /^[A-Z]{1,2}$/u }).click();
    await page.getByRole("menuitem", { name: "Admin console" }).click();
    await expect(page).toHaveURL(/\/admin\//u);
  });

  test("the saved session belongs to the administrator", async ({
    request,
  }) => {
    const response = await request.get("/api/v1/auth/session");
    expect(response.ok()).toBe(true);
    expect(await response.json()).toMatchObject({
      username: ADMIN_USERNAME,
    });
  });
});
