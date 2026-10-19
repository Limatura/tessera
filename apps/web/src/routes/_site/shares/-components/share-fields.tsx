import type { components } from "@tessera/api/schema";
import { Badge } from "@tessera/ui/components/badge";
import { ShieldAlert, ShieldEllipsis } from "lucide-react";

import { isExpired } from "@/lib/expiry-label";
import * as m from "@/paraglide/messages";

export type Share = components["schemas"]["ShareResponse"];

/**
 * Whether a share can still be retrieved. Switched off comes first: `blocked`
 * when the malware scanner found a threat, `disabled` when an operator did it.
 * Otherwise inferred from expiry and remaining retrievals: either ends the
 * link, and a dead share's files are deleted on the next sweep while the record
 * stays a while longer.
 */
export type ShareState =
  | "live"
  | "expired"
  | "exhausted"
  | "blocked"
  | "disabled";

export function shareState(share: Share): ShareState {
  if (share.disabled) {
    return share.scan_state === "infected" ? "blocked" : "disabled";
  }
  if (isExpired(share.expires_at)) {
    return "expired";
  }
  if (share.uses_remaining === 0) {
    return "exhausted";
  }
  return "live";
}

/**
 * What the owner would recognise it by: its title, or the file it holds. `null` when
 * there is neither, a note without a title or an encrypted share, whose names are
 * sealed and never reach the server in the clear; the code stands in for those.
 */
export function shareName(share: Share): string | null {
  if (share.title) {
    return share.title;
  }
  const [first] = share.files;
  if (!first?.filename) {
    return null;
  }
  return share.files.length > 1
    ? m.shares_name_more_files({
        count: share.files.length,
        name: first.filename,
      })
    : first.filename;
}

/** Why it can no longer be retrieved. Nothing for a live share: the normal state needs no label. */
export function ShareStateBadge({ state }: { state: ShareState }) {
  switch (state) {
    case "live": {
      return null;
    }
    case "blocked": {
      return (
        <Badge variant="destructive">
          <ShieldAlert data-icon="inline-start" />
          {m.shares_status_blocked()}
        </Badge>
      );
    }
    case "disabled": {
      return <Badge variant="outline">{m.shares_status_disabled()}</Badge>;
    }
    case "expired": {
      return <Badge variant="outline">{m.all_shares_status_expired()}</Badge>;
    }
    case "exhausted": {
      return <Badge variant="outline">{m.all_shares_status_exhausted()}</Badge>;
    }
    default: {
      return null;
    }
  }
}

/** Kind and password badges shared by the table and the mobile list, so both show the same. */
export function ShareBadges({ share }: { share: Share }) {
  return (
    <span className="flex flex-wrap gap-1">
      <Badge variant="secondary">{shareKindLabel(share.kind)}</Badge>
      {share.has_password && (
        <Badge variant="outline">{m.shares_needs_password()}</Badge>
      )}
      {share.scanning && (
        <Badge
          className="border-amber-500/50 text-amber-700 dark:text-amber-400"
          variant="outline"
        >
          <ShieldEllipsis className="animate-pulse" data-icon="inline-start" />
          {m.shares_status_scanning()}
        </Badge>
      )}
    </span>
  );
}

/**
 * The API's `kind` is an English code, translated for display. Unknown values show
 * as-is, so a kind added on the backend is never blank.
 */
function shareKindLabel(kind: string): string {
  switch (kind) {
    case "text": {
      return m.shares_kind_text();
    }
    case "file": {
      return m.shares_kind_file();
    }
    case "link": {
      return m.shares_kind_link();
    }
    default: {
      return kind;
    }
  }
}
