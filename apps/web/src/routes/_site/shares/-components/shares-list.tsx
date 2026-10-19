import { Link } from "@tanstack/react-router";
import { Checkbox } from "@tessera/ui/components/checkbox";
import { cn } from "@tessera/ui/lib/utils";

import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { RowSelection } from "@/hooks/use-row-selection";
import { expiryLabel } from "@/lib/expiry-label";
import * as m from "@/paraglide/messages";

import {
  ShareBadges,
  ShareStateBadge,
  shareName,
  shareState,
} from "./share-fields";
import type { Share } from "./share-fields";
import { ShareRowActions } from "./share-row-actions";

/**
 * The narrow-screen layout: one card per share.
 *
 * The top half is what someone checking the list on a phone wants to know: what it is
 * (its name, or the code when it has none), its kind and code, and on the right whether
 * it still works and how long it has left. Full dates drop to the bottom half, and the
 * buttons sit in the top right corner instead of taking width from the content.
 *
 * The card is a link (stretched with `after:`), with the checkbox and buttons raised
 * above it with `relative`, since a `<button>` cannot go inside an `<a>`. The checkbox
 * sits left of the name, aligned with the first line: within thumb reach, but away from
 * the delete button in the corner.
 *
 * Dead cards recede (faint background, dashed border, faded name) and the
 * top right shows why. Not red: expiring is a normal part of the lifecycle, not an error, and red
 * is kept for things that need action.
 */
export function SharesList({
  shares,
  selection,
  onManage,
  onDelete,
}: {
  shares: Share[];
  selection: RowSelection;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  return (
    <ul className="flex flex-col gap-3">
      {shares.map((share) => (
        <ShareCard
          key={share.id}
          onDelete={onDelete}
          onManage={onManage}
          onSelectedChange={(checked) => selection.setOne(share.code, checked)}
          selected={selection.isSelected(share.code)}
          share={share}
        />
      ))}
    </ul>
  );
}

function ShareCard({
  share,
  selected,
  onSelectedChange,
  onManage,
  onDelete,
}: {
  share: Share;
  selected: boolean;
  onSelectedChange: (checked: boolean) => void;
  onManage: (share: Share) => void;
  onDelete: (share: Share) => void;
}) {
  const formatDate = useDateTimeFormatter();
  const state = shareState(share);
  const dead = state !== "live";
  const name = shareName(share);

  return (
    <li
      className={cn(
        "relative rounded-xl border bg-card transition-colors focus-within:ring-2 focus-within:ring-ring active:bg-muted/40",
        dead && "border-dashed bg-muted/30",
        selected && "border-solid border-primary bg-muted/50"
      )}
    >
      <div className="flex items-start gap-3 p-4 pb-3">
        <Checkbox
          aria-label={m.selection_select_row({ label: share.code })}
          checked={selected}
          className="relative z-10 mt-1.5"
          onCheckedChange={onSelectedChange}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <div className="flex items-start gap-2">
            <Link
              className={cn(
                "min-w-0 flex-1 truncate font-medium outline-none after:absolute after:inset-0 after:rounded-xl",
                !name && "font-mono text-lg",
                dead && "text-muted-foreground"
              )}
              params={{ code: share.code }}
              to="/s/$code"
            >
              {name ?? share.code}
            </Link>
            <div className="relative z-10 -mt-2 -mr-2 shrink-0">
              <ShareRowActions
                onDelete={onDelete}
                onManage={onManage}
                share={share}
              />
            </div>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span
              className={cn(
                "flex min-w-0 items-center gap-2",
                dead && "opacity-60"
              )}
            >
              <ShareBadges share={share} />
              {name && (
                <span className="truncate font-mono text-muted-foreground text-xs">
                  {share.code}
                </span>
              )}
            </span>
            {dead ? (
              <ShareStateBadge state={state} />
            ) : (
              <span className="shrink-0 font-medium text-sm">
                {expiryLabel(share.expires_at)}
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="border-t py-3 pr-4 pl-11">
        <dl className="flex flex-wrap gap-x-4 gap-y-0.5 text-muted-foreground text-xs tabular-nums">
          {share.expires_at && (
            <Meta label={m.shares_col_expires()}>
              {formatDate(share.expires_at)}
            </Meta>
          )}
          <Meta label={m.shares_col_created()}>
            {formatDate(share.created_at)}
          </Meta>
          <Meta label={m.shares_col_accessed()}>
            {formatDate(share.last_accessed_at)}
          </Meta>
        </dl>
      </div>
    </li>
  );
}

function Meta({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex gap-1.5">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}
