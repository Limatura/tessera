import { browser } from "#imports";

/** Notification ids that open a link when clicked carry it after this prefix. */
export const LINK_NOTIFICATION_PREFIX = "link:";

/**
 * A system notification. With `link`, clicking it opens the link (see the
 * background's `notifications.onClicked`).
 */
export async function notify(
  title: string,
  message: string,
  link?: string
): Promise<void> {
  const id = link
    ? `${LINK_NOTIFICATION_PREFIX}${crypto.randomUUID()}|${link}`
    : crypto.randomUUID();
  await browser.notifications.create(id, {
    iconUrl: browser.runtime.getURL("/icon/128.png"),
    message,
    title,
    type: "basic",
  });
}

/** The link a notification id carries, if it is one of ours. */
export function linkOf(notificationId: string): string | undefined {
  if (!notificationId.startsWith(LINK_NOTIFICATION_PREFIX)) {
    return undefined;
  }
  const separator = notificationId.indexOf("|");
  return separator === -1 ? undefined : notificationId.slice(separator + 1);
}
