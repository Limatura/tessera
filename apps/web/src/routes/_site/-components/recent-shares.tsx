import { IconExternalLink } from "@tabler/icons-react";
import { Button } from "@tessera/ui/components/button";
import { Settings2 } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";

import { expiryLabel } from "@/lib/expiry-label";
import { minutesUntil } from "@/lib/upload/share-api";
import * as m from "@/paraglide/messages";
import { useRecentSharesStore } from "@/stores/recent-shares";
import type { RecentShare } from "@/stores/recent-shares";

import { ManageShareDialog } from "./manage-share-dialog";
import type { ManagedShare } from "./manage-share-dialog";
import { expiresAtFrom } from "./share-settings-panel";

/**
 * How many rows show when collapsed. The list is for finding what was just uploaded;
 * longer gets in the way.
 */
const COLLAPSED_COUNT = 3;

/**
 * Below the home page upload card: shares this browser uploaded anonymously
 * and still holds a management token for.
 *
 * Once the result screen is closed, this is the only way back for an
 * anonymous uploader to change settings or delete a share. With no records
 * nothing is drawn, so a first-time visitor never sees an empty list.
 */
export function RecentShares() {
  const shares = useRecentSharesStore((state) => state.shares);
  const update = useRecentSharesStore((state) => state.update);
  const forget = useRecentSharesStore((state) => state.forget);
  const [expanded, setExpanded] = useState(false);
  const [managing, setManaging] = useState<ManagedShare | null>(null);
  const [open, setOpen] = useState(false);

  if (shares.length === 0) {
    return null;
  }

  const handleManage = (share: RecentShare) => {
    setManaging(toManaged(share));
    setOpen(true);
  };

  return (
    <section
      aria-labelledby="recent-shares-heading"
      className="mt-4 rounded-xl border border-border/60 bg-card/60 p-3 backdrop-blur-2xl"
    >
      <div className="flex items-baseline justify-between gap-2 px-1 pb-2">
        <h2 className="font-medium text-sm" id="recent-shares-heading">
          {m.recent_shares_title()}
        </h2>
        <p className="text-muted-foreground text-xs">
          {m.recent_shares_hint()}
        </p>
      </div>

      <ul className="flex flex-col">
        {shares.slice(0, COLLAPSED_COUNT).map((share) => (
          <RecentShareRow
            key={share.code}
            onManage={handleManage}
            share={share}
          />
        ))}
      </ul>
      {/* Rows past the fold grow and shrink in, rather than popping */}
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.ul
            animate={{ height: "auto", opacity: 1 }}
            className="flex flex-col overflow-hidden"
            exit={{ height: 0, opacity: 0 }}
            initial={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25, ease: [0.16, 1, 0.3, 1] }}
          >
            {shares.slice(COLLAPSED_COUNT).map((share) => (
              <RecentShareRow
                key={share.code}
                onManage={handleManage}
                share={share}
              />
            ))}
          </motion.ul>
        )}
      </AnimatePresence>

      {shares.length > COLLAPSED_COUNT && (
        <Button
          className="mt-1 w-full text-muted-foreground"
          onClick={() => setExpanded((value) => !value)}
          size="sm"
          type="button"
          variant="ghost"
        >
          {expanded
            ? m.recent_shares_show_less()
            : m.recent_shares_show_all({ count: shares.length })}
        </Button>
      )}

      <ManageShareDialog
        onDeleted={forget}
        onForget={forget}
        onOpenChange={setOpen}
        onSettingsChange={(code, snapshot) =>
          update(code, {
            expiresAt: expiresAtFrom(snapshot.expiresInMinutes),
            hasPassword: snapshot.hasPassword,
            usesRemaining: snapshot.burnAfterReading ? 1 : null,
          })
        }
        open={open}
        share={managing}
        verify
      />
    </section>
  );
}

function RecentShareRow({
  onManage,
  share,
}: {
  onManage: (share: RecentShare) => void;
  share: RecentShare;
}) {
  return (
    <li className="flex items-center gap-1">
      <button
        className="flex min-w-0 flex-1 flex-col rounded-lg px-2 py-2 text-left transition-colors hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        onClick={() => onManage(share)}
        type="button"
      >
        <span className="truncate text-sm">{share.fileName}</span>
        <span className="text-muted-foreground text-xs">
          <span className="font-mono">{share.code}</span>
          {" · "}
          {expiryLabel(share.expiresAt)}
        </span>
      </button>
      {/* Siblings of the row button, not inside it: a link cannot nest in a button */}
      <Button
        className="shrink-0 text-muted-foreground"
        nativeButton={false}
        render={
          <a
            aria-label={m.upload_result_open()}
            href={share.shortUrl}
            rel="noopener"
            target="_blank"
          />
        }
        size="icon-sm"
        variant="ghost"
      >
        <IconExternalLink />
      </Button>
      <Button
        aria-label={m.shares_manage_aria({ code: share.code })}
        className="shrink-0 text-muted-foreground"
        onClick={() => onManage(share)}
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <Settings2 />
      </Button>
    </li>
  );
}

function toManaged(share: RecentShare): ManagedShare {
  return {
    code: share.code,
    initial: {
      burnAfterReading: share.usesRemaining === 1,
      expiresInMinutes: minutesUntil(share.expiresAt),
      hasPassword: share.hasPassword,
      passwordIsKey: share.passwordIsKey,
    },
    managementToken: share.managementToken,
    shortUrl: share.shortUrl,
    title: share.fileName,
  };
}
