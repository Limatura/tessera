import type { components } from "@tessera/api/schema";
import { formatBytes } from "@tessera/ui/lib/format";

import { isDeletedUsername } from "@/lib/admin/accounts";
import * as m from "@/paraglide/messages";

export type AdminShare = components["schemas"]["AdminShareResponse"];

/** 單一檔案時用檔名;文字、連結跟多檔都沒有檔名,改說它是什麼。 */
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

/** null 是匿名上傳;"gone-" 開頭是帳號已被刪除,兩者都跟真的帳號名分開顯示。 */
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
 * 過期、次數用完:正常生命週期走完,檔案下一次清掃就刪,紀錄再留一段時間。
 * `disabled`(掃毒擋下)不算 —— 那是要有人處理的事,不該跟著退成背景,
 * 它的檔案也要等保留期滿才刪。
 */
export function isDeadShare(share: AdminShare): boolean {
  return share.status === "expired" || share.status === "exhausted";
}

/**
 * 大小欄。內容被清掃刪掉後 `size_bytes` 還是原本的大小,直接顯示會像還佔著空間;
 * 改說「已清除」,原本多大放括號裡 —— 看得出清掉的是什麼規模的東西。
 */
export function sizeLabel(share: AdminShare): string {
  const size = formatBytes(share.size_bytes);
  return share.content_removed ? m.all_shares_size_removed({ size }) : size;
}
