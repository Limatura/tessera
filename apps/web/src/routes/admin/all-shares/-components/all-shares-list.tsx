import { IconTrash } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Checkbox } from "@tessera/ui/components/checkbox";
import { formatBytes } from "@tessera/ui/lib/format";
import { cn } from "@tessera/ui/lib/utils";

import { PurgeDate } from "@/components/admin/purge-date";
import { ShareStatusBadge } from "@/components/admin/share-status-badge";
import { useDateTimeFormatter } from "@/hooks/use-date-time-formatter";
import type { RowSelection } from "@/hooks/use-row-selection";
import {
  isDeadShare,
  ownerLabel,
  shareLabel,
  sizeLabel,
} from "@/lib/admin/admin-shares";
import type { AdminShare } from "@/lib/admin/admin-shares";
import * as m from "@/paraglide/messages";

/**
 * The narrow-screen layout: one card per share.
 *
 * Here the admin decides what to clean up, so the content name and status are large;
 * owner, size, bytes served and creation time sit on a line below. Share codes are
 * hidden from the admin, so a card is not a link: tapping it toggles its checkbox, an
 * easier target on a phone during a batch cleanup than a small box.
 *
 * Expired and used-up cards recede (faint background, dashed border, faded name); ones
 * disabled by the malware scanner do not.
 */
export function AllSharesList({
  shares,
  selection,
  onDelete,
}: {
  shares: AdminShare[];
  selection: RowSelection | null;
  onDelete: (share: AdminShare) => void;
}) {
  return (
    <ul className="flex flex-col gap-3">
      {shares.map((share) => (
        <ShareCard
          key={share.id}
          onDelete={selection ? () => onDelete(share) : null}
          onSelectedChange={
            selection ? (checked) => selection.setOne(share.id, checked) : null
          }
          selected={selection?.isSelected(share.id) ?? false}
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
  onDelete,
}: {
  share: AdminShare;
  selected: boolean;
  /** `null`: no delete permission, so no checkboxes. */
  onSelectedChange: ((checked: boolean) => void) | null;
  onDelete: (() => void) | null;
}) {
  const formatDate = useDateTimeFormatter();
  const label = shareLabel(share);
  const dead = isDeadShare(share);

  return (
    <li
      className={cn(
        "relative rounded-xl border bg-card transition-colors",
        dead && "border-dashed bg-muted/30",
        onSelectedChange && "has-[label:active]:bg-muted/40",
        selected && "border-solid border-primary bg-muted/50"
      )}
    >
      <div className="flex items-start gap-3 p-4 pb-3">
        {onSelectedChange && (
          // The card is the checkbox's label: `after:` stretches it over the card,
          // and the delete button sits above it with z-10.
          // oxlint-disable-next-line jsx-a11y/label-has-associated-control -- Checkbox is a base-ui button; wrapping it in a label associates them
          <label className="mt-0.5 flex after:absolute after:inset-0 after:rounded-xl">
            <Checkbox
              aria-label={m.selection_select_row({ label })}
              checked={selected}
              onCheckedChange={onSelectedChange}
            />
          </label>
        )}
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <p
            className={cn(
              "truncate font-medium",
              dead && "text-muted-foreground"
            )}
          >
            {label}
          </p>
          <span className="flex flex-wrap items-center gap-2">
            <ShareStatusBadge share={share} />
            <PurgeDate share={share} />
          </span>
        </div>
        {onDelete && (
          <Button
            aria-label={m.all_shares_delete_aria({ label })}
            className="relative z-10 -mt-2 -mr-2 shrink-0"
            onClick={onDelete}
            size="icon"
            type="button"
            variant="ghost"
          >
            <IconTrash />
          </Button>
        )}
      </div>

      <dl
        className={cn(
          "grid grid-cols-2 gap-x-4 gap-y-1 border-t py-3 pr-4 text-xs tabular-nums",
          onSelectedChange ? "pl-11" : "pl-4"
        )}
      >
        <Meta label={m.all_shares_col_owner()}>{ownerLabel(share)}</Meta>
        <Meta label={m.all_shares_col_created()}>
          {formatDate(share.created_at)}
        </Meta>
        <Meta label={m.all_shares_col_size()}>{sizeLabel(share)}</Meta>
        <Meta label={m.all_shares_col_served()}>
          {formatBytes(share.bytes_served)}
        </Meta>
      </dl>
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
    <div className="flex min-w-0 gap-1.5">
      <dt className="shrink-0 text-muted-foreground">{label}</dt>
      <dd className="truncate">{children}</dd>
    </div>
  );
}
