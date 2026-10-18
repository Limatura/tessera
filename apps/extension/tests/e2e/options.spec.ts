import { expect, test } from "./fixtures";
import { env, setEncryption } from "./instance";

test.describe("settings", () => {
  test("the instance is set through the form, and encryption is only offered once the operator allows it", async ({
    extension,
  }) => {
    await setEncryption(false);
    const options = await extension.open("options");
    await options
      .getByPlaceholder("https://files.example.com")
      .fill(env().server);
    await options.getByRole("button", { name: "Save and connect" }).click();

    await expect(options.getByText(/Connected to Tessera/u)).toBeVisible();
    await expect(
      options.getByText(/End-to-end encryption not offered/u)
    ).toBeVisible();
    await expect(
      options.getByRole("switch", { name: "End-to-end encryption" })
    ).toHaveCount(0);

    await setEncryption(true);
    await options.reload();
    const encrypt = options.getByRole("switch", {
      name: "End-to-end encryption",
    });
    await expect(encrypt).toBeVisible();
    await encrypt.click();
    await expect(encrypt).toBeChecked();
    expect(await extension.stored("sync", "defaults")).toMatchObject({
      encrypt: true,
    });
  });

  test("a pasted share link is cut back to the instance's address", async ({
    extension,
  }) => {
    const options = await extension.open("options");
    await options
      .getByPlaceholder("https://files.example.com")
      .fill(`${env().server}/s/ABCDEFGH#key`);
    await options.getByRole("button", { name: "Save and connect" }).click();
    await expect(options.getByText(/Connected to Tessera/u)).toBeVisible();
    expect(await extension.stored("local", "server")).toBe(env().server);
  });
});
