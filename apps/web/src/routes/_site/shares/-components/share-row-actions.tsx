import {
  IconAdjustmentsHorizontal,
  IconExternalLink,
  IconTrash,
} from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { Button } from "@tessera/ui/components/button";

import * as m from "@/paraglide/messages";
import { useShareKeysStore } from "@/stores/share-keys";

import { shareState } from "./share-fields";
import type { Share } from "./share-fields";

/**
 * The buttons shared by the table and the mobile list: open in a new tab, edit settings and delete.
 * The actions themselves are in `ManageShareDialog` and `DeleteShareDialog`.
 *
 * A dead share cannot be edited: its files go on the next sweep after it dies, so
 * extending it or adding retrievals gets a 410 (`share_content_removed`) and the dialog
 * would only hit a wall. Delete stays, as the way to clear the record.
 */
export function ShareRowActions({
  share,
  onManage,
  onDelete,
}: {
  share: Share;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  // A link-encrypted share opens only with the key after `#`, which this
  // browser has if it uploaded the share.
  const key = useShareKeysStore((state) =>
    share.encryption === "link" ? state.keys[share.code] : undefined
  );
  const live = shareState(share) === "live";

  return (
    <span className="flex justify-end gap-1">
      {live && (
        <Button
          aria-label={m.shares_open_aria({ code: share.code })}
          nativeButton={false}
          render={
            <Link
              hash={key}
              params={{ code: share.code }}
              rel="noopener"
              target="_blank"
              to="/s/$code"
            />
          }
          size="icon"
          variant="ghost"
        >
          <IconExternalLink />
        </Button>
      )}
      {live && (
        <Button
          aria-label={m.shares_manage_aria({ code: share.code })}
          onClick={() => onManage(share)}
          size="icon"
          type="button"
          variant="ghost"
        >
          <IconAdjustmentsHorizontal />
        </Button>
      )}
      <Button
        aria-label={m.shares_delete_aria({ code: share.code })}
        onClick={() => onDelete(share)}
        size="icon"
        type="button"
        variant="ghost"
      >
        <IconTrash />
      </Button>
    </span>
  );
}
