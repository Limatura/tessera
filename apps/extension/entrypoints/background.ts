import { i18n } from "#i18n";
import { browser, defineBackground } from "#imports";
import { watchSignIn } from "@/lib/auth";
import {
  abandonJobs,
  isActive,
  listJobs,
  pruneJobs,
  watchJobs,
} from "@/lib/jobs";
import { createMenus, handleMenuClick } from "@/lib/menus";
import { isMessage } from "@/lib/messages";
import { linkOf, notify } from "@/lib/notify";
import type { Job } from "@/lib/storage";
import { cancelUpload, describe, runUpload } from "@/lib/upload";

/**
 * Chrome stops an idle service worker after about 30 seconds, and a request
 * still on the wire does not count as activity. While an upload runs here, a
 * cheap extension call every 20 seconds keeps it awake.
 */
const KEEPALIVE_MS = 20_000;

/** The badge behind the count of running uploads: the site's primary orange. */
const BADGE_COLOR = "#c2410c";

export default defineBackground(() => {
  const bootedAt = Date.now();

  // No browser API can click a native context menu, so the end-to-end suite
  // (built with `--mode e2e`) calls the click handler itself. The condition is
  // a build-time constant: every other build drops this line.
  if (import.meta.env.MODE === "e2e") {
    Object.assign(globalThis, { tesseraMenuClick: handleMenuClick });
  }

  browser.runtime.onInstalled.addListener(() => {
    void guard(createMenus);
  });
  browser.runtime.onStartup.addListener(() => {
    void guard(createMenus);
  });

  browser.contextMenus.onClicked.addListener((info, tab) => {
    void guard(() => handleMenuClick(info, tab));
  });

  browser.notifications.onClicked.addListener((id) => {
    void guard(async () => {
      const link = linkOf(id);
      if (link) {
        await browser.tabs.create({ url: link });
      }
      await browser.notifications.clear(id);
    });
  });

  browser.runtime.onMessage.addListener((message: unknown) => {
    if (!isMessage(message)) {
      return;
    }
    if (message.type === "upload-note") {
      // Not awaited: the popup that asked may close, and the job reports
      // through storage.
      void guard(() =>
        runUpload({ text: message.text, title: message.title, type: "note" })
      );
    } else if (message.type === "cancel-upload") {
      cancelUpload(message.id);
    }
  });

  watchSignIn();

  // Whatever this worker was uploading before it was stopped is lost.
  void guard(() => abandonJobs(bootedAt, i18n.t("errors.interrupted")));

  let keepalive: ReturnType<typeof setInterval> | undefined;
  const handleJobs = async (jobs: Job[]) => {
    const active = jobs.filter(isActive);
    await browser.action.setBadgeBackgroundColor({ color: BADGE_COLOR });
    await browser.action.setBadgeText({
      text: active.length > 0 ? String(active.length) : "",
    });

    if (active.length > 0 && keepalive === undefined) {
      keepalive = setInterval(() => {
        void guard(() => browser.runtime.getPlatformInfo());
      }, KEEPALIVE_MS);
    } else if (active.length === 0 && keepalive !== undefined) {
      clearInterval(keepalive);
      keepalive = undefined;
      await pruneJobs();
    }
  };
  watchJobs((jobs) => {
    void guard(() => handleJobs(jobs));
  });
  void guard(async () => handleJobs(await listJobs()));
});

/**
 * Runs `task`, saying so if it fails. The background has no screen, so
 * anything that goes wrong outside an upload (which reports its own
 * failures) becomes a notification instead of vanishing.
 */
async function guard(task: () => Promise<unknown>): Promise<void> {
  try {
    await task();
  } catch (error) {
    try {
      await notify(i18n.t("errors.unknownTitle"), describe(error));
    } catch {
      // Not even a notification: nothing further to tell anyone.
    }
  }
}
