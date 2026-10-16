import type { components } from "@tessera/api/schema";
import { formatBytes } from "@tessera/ui/lib/format";

import { isDeletedUsername } from "@/lib/admin/accounts";
import * as m from "@/paraglide/messages";

export type AdminShare = components["schemas"]["AdminShareResponse"];

/** A single file uses its name; text, links and multi-file shares have none, so say what it is. */
export function shareLabel(share: AdminShare): string {
  if (share.filename) {
    return share.filename;
  }
  if (share.kind === "text") {
    return m.all_shares_label_text();
  }
  if (share.kind === "link") {
    return m.all_shares_label_link();
  }
  return m.all_shares_label_file_count({ count: share.file_count });
}

/**
 * null is an anonymous upload and a "gone-" prefix is a deleted account; both
 * are shown apart from real usernames.
 */
export function ownerLabel(share: AdminShare): string {
  const { owner_username: ownerUsername } = share;
  if (!ownerUsername) {
    return m.all_shares_owner_anonymous();
  }
  if (isDeletedUsername(ownerUsername)) {
    return m.all_shares_owner_deleted();
  }
  return ownerUsername;
}

/**
 * Expired or out of downloads: the normal lifecycle has ended, the files go on
 * the next sweep and the record stays a while longer. `disabled` (stopped by
 * the malware scanner) does not count: it needs someone to act and should not
 * fade into the background, and its files are kept until the retention period
 * ends.
 */
export function isDeadShare(share: AdminShare): boolean {
  return share.status === "expired" || share.status === "exhausted";
}

/**
 * The size column. After the sweep removes the content, `size_bytes` still
 * holds the original size, which would look like space still in use. Say
 * "purged" instead, with the original size in brackets to show the scale.
 */
export function sizeLabel(share: AdminShare): string {
  const size = formatBytes(share.size_bytes);
  return share.content_removed ? m.all_shares_size_removed({ size }) : size;
}
