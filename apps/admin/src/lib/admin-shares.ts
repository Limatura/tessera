import type { components } from "@tessera/api/schema";

import { isDeletedUsername } from "@/lib/accounts";
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
