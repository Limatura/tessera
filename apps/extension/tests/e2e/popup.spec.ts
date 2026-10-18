import { expect, test } from "./fixtures";
import { describeShare, env, setEncryption } from "./instance";

test.describe("popup", () => {
  test("a note is uploaded by the background, anonymously, and listed from this browser's record", async ({
    extension,
  }) => {
    await setEncryption(false);
    await extension.useTestServer();
    const popup = await extension.open("popup");
    await expect(popup.getByText("Anonymous", { exact: true })).toBeVisible();
    // Encryption is off on the instance: no lock.
    await expect(
      popup.getByTitle("Uploads are end-to-end encrypted by default")
    ).toHaveCount(0);

    const before = await extension.jobs();
    await popup.getByRole("button", { name: "Upload text" }).click();
    await popup
      .getByLabel("Upload text")
      .fill("hello from the extension\nline two");
    await popup.getByRole("button", { name: "Upload note" }).click();
    const job = await extension.nextJob(before);

    expect(job).toMatchObject({
      kind: "note",
      state: "done",
    });
    expect(job.link).not.toContain("#");
    expect(await describeShare(job.link ?? "")).toMatchObject({
      encryption: null,
      kind: "text",
    });

    await popup.getByRole("tab", { name: "My shares" }).click();
    await expect(
      popup.getByRole("tabpanel").getByText("hello from the extension")
    ).toBeVisible();
  });

  test("with encryption on, the key travels in the link and the site opens the note with it", async ({
    context,
    extension,
  }) => {
    await setEncryption(true);
    await extension.useTestServer();
    const options = await extension.open("options");
    await options
      .getByRole("switch", { name: "End-to-end encryption" })
      .click();
    await options.close();

    const popup = await extension.open("popup");
    // The green lock beside the account says uploads are sealed by default.
    await expect(
      popup.getByTitle("Uploads are end-to-end encrypted by default")
    ).toBeVisible();
    const before = await extension.jobs();
    await popup.getByRole("button", { name: "Upload text" }).click();
    await popup.getByLabel("Upload text").fill("secret note body");
    await popup.getByRole("button", { name: "Upload note" }).click();
    const job = await extension.nextJob(before);

    expect(job.state, job.error).toBe("done");
    expect(job.link).toContain("#");
    expect(await describeShare(job.link ?? "")).toMatchObject({
      encryption: "link",
    });

    const viewer = await context.newPage();
    await viewer.goto(job.link ?? "");
    await expect(viewer.getByText("secret note body").first()).toBeVisible({
      timeout: 20_000,
    });
  });

  test("Upload files opens the site's own upload page", async ({
    context,
    extension,
  }) => {
    await extension.useTestServer();
    const popup = await extension.open("popup");
    const [site] = await Promise.all([
      context.waitForEvent("page"),
      popup.getByRole("button", { name: "Upload files" }).click(),
    ]);
    await site.waitForURL(`${env().server}/`);
  });

  test("burn after reading, set as a default, leaves a share one retrieval", async ({
    extension,
  }) => {
    await setEncryption(false);
    await extension.useTestServer();
    const options = await extension.open("options");
    const burn = options.getByRole("switch", { name: "Burn after reading" });
    await burn.click();
    await expect(burn).toBeChecked();
    await options.close();

    const popup = await extension.open("popup");
    await expect(popup.getByText(/Burn after reading/u)).toBeVisible();
    const before = await extension.jobs();
    await popup.getByRole("button", { name: "Upload text" }).click();
    await popup.getByLabel("Upload text").fill("read me once");
    await popup.getByRole("button", { name: "Upload note" }).click();
    const job = await extension.nextJob(before);
    expect(job.state, job.error).toBe("done");
    const share = await describeShare(job.link ?? "");
    expect(share?.uses_remaining).toBe(1);
  });
});
