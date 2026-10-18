/**
 * The right-click menu: one "Tessera" entry, whose items are the ones that
 * fit what was clicked (an image, a link, a selection, the page itself).
 */
import { i18n } from "#i18n";
import { browser } from "#imports";
import type { Browser } from "#imports";

import { notify } from "./notify";
import { readSelection } from "./page-content";
import type { ClickedIn } from "./page-content";
import { serverItem } from "./storage";
import { runUpload } from "./upload";
import type { UploadSource } from "./upload";

const ROOT = "tessera";

type MenuId =
  | "upload-image"
  | "upload-video"
  | "upload-audio"
  | "upload-linked-file"
  | "share-link"
  | "upload-selection"
  | "share-page";

type Contexts = NonNullable<Browser.contextMenus.CreateProperties["contexts"]>;

const ITEMS: {
  id: MenuId;
  contexts: Contexts;
  title: () => string;
}[] = [
  {
    contexts: ["image"],
    id: "upload-image",
    title: () => i18n.t("menu.uploadImage"),
  },
  {
    contexts: ["video"],
    id: "upload-video",
    title: () => i18n.t("menu.uploadVideo"),
  },
  {
    contexts: ["audio"],
    id: "upload-audio",
    title: () => i18n.t("menu.uploadAudio"),
  },
  {
    contexts: ["selection"],
    id: "upload-selection",
    title: () => i18n.t("menu.uploadSelection"),
  },
  {
    contexts: ["link"],
    id: "upload-linked-file",
    title: () => i18n.t("menu.uploadLinkedFile"),
  },
  {
    contexts: ["link"],
    id: "share-link",
    title: () => i18n.t("menu.shareLink"),
  },
  {
    contexts: ["page"],
    id: "share-page",
    title: () => i18n.t("menu.sharePage"),
  },
];

/**
 * (Re)creates the menu. Menus outlive the service worker in Chrome but not
 * every restart in Firefox, so this runs on install and on every start, and
 * clears first so the two never stack up.
 */
export async function createMenus(): Promise<void> {
  await browser.contextMenus.removeAll();
  browser.contextMenus.create({
    contexts: ["page", "selection", "link", "image", "video", "audio"],
    id: ROOT,
    title: i18n.t("menu.root"),
  });
  for (const item of ITEMS) {
    browser.contextMenus.create({
      contexts: item.contexts,
      id: item.id,
      parentId: ROOT,
      title: item.title(),
    });
  }
}

export async function handleMenuClick(
  info: Browser.contextMenus.OnClickData,
  tab: Browser.tabs.Tab | undefined
): Promise<void> {
  if (!(await serverItem.getValue())) {
    await notify(i18n.t("errors.noServerTitle"), i18n.t("errors.noServer"));
    await browser.runtime.openOptionsPage();
    return;
  }
  const clickedIn: ClickedIn | undefined =
    tab?.id === undefined
      ? undefined
      : { frameId: info.frameId ?? 0, tabId: tab.id };
  const source = await sourceFor(
    info.menuItemId as MenuId,
    info,
    tab,
    clickedIn
  );
  if (source) {
    await runUpload(source);
  }
}

async function sourceFor(
  id: MenuId,
  info: Browser.contextMenus.OnClickData,
  tab: Browser.tabs.Tab | undefined,
  clickedIn: ClickedIn | undefined
): Promise<UploadSource | undefined> {
  switch (id) {
    case "upload-image":
    case "upload-video":
    case "upload-audio": {
      return info.srcUrl
        ? { clickedIn, type: "media", url: info.srcUrl }
        : undefined;
    }
    case "upload-linked-file": {
      return info.linkUrl
        ? { clickedIn, type: "media", url: info.linkUrl }
        : undefined;
    }
    case "share-link": {
      return info.linkUrl ? { type: "link", url: info.linkUrl } : undefined;
    }
    case "upload-selection": {
      const { text, language } = await readSelection(
        clickedIn,
        info.selectionText ?? ""
      );
      return text.trim()
        ? { language, text, title: tab?.title || undefined, type: "note" }
        : undefined;
    }
    case "share-page": {
      const url = info.pageUrl ?? tab?.url;
      return url ? { type: "link", url } : undefined;
    }
    default: {
      return undefined;
    }
  }
}
