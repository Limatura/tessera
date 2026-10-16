import { IconInfoCircle } from "@tabler/icons-react";

/**
 * Explains why dead shares are still on the page. Without it, an expired share
 * still in the list looks like broken cleanup.
 *
 * The two pages need different depth, so the text comes from the caller. "My
 * shares" only needs to know they disappear on their own and can be deleted
 * sooner. The admin page needs to know the files no longer use space and why
 * the record stays (`share.retain_after_death_hours`); each row's removal date
 * is shown next to its status by `PurgeDate`.
 */
export function DeadSharesNote({ children }: { children: React.ReactNode }) {
  return (
    <p className="flex items-start gap-2 text-muted-foreground text-xs">
      <IconInfoCircle className="mt-px shrink-0" size={14} />
      {children}
    </p>
  );
}
