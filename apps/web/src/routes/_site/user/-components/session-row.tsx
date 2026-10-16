import {
  IconDeviceDesktop,
  IconDeviceMobile,
  IconTerminal2,
} from "@tabler/icons-react";
import { Badge } from "@tessera/ui/components/badge";
import { Button } from "@tessera/ui/components/button";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@tessera/ui/components/responsive-dialog";
import { cn } from "@tessera/ui/lib/utils";
import type * as React from "react";
import { useState } from "react";

import type { Session } from "@/hooks/use-sessions";
import { formatUserAgent, parseUserAgent } from "@/lib/user-agent";
import * as m from "@/paraglide/messages";

/**
 * `ended_reason` is a fixed backend string. An unknown value (added on the backend,
 * not handled here yet) is shown as-is, which beats hiding the answer.
 */
function endedReasonLabel(reason: string): string {
  switch (reason) {
    case "signed_out": {
      return m.user_sessions_ended_signed_out();
    }
    case "revoked": {
      return m.user_sessions_ended_revoked();
    }
    case "password_changed": {
      return m.user_sessions_ended_password_changed();
    }
    case "password_reset": {
      return m.user_sessions_ended_password_reset();
    }
    case "account_disabled": {
      return m.user_sessions_ended_account_disabled();
    }
    default: {
      return reason;
    }
  }
}

function DeviceIcon({ userAgent }: { userAgent: string | null | undefined }) {
  const parsed = parseUserAgent(userAgent);

  if (parsed?.browser === "curl" || parsed?.browser === "Tessera CLI") {
    return <IconTerminal2 size={18} />;
  }
  if (parsed?.os === "iOS" || parsed?.os === "Android") {
    return <IconDeviceMobile size={18} />;
  }
  return <IconDeviceDesktop size={18} />;
}

/**
 * One row of the meta list. On phones the label sits left and the value right, stacked
 * into a small table that is easy to scan; on desktop (sm+) the outer `dl` becomes
 * flex-wrap with compact "label value" pairs side by side.
 *
 * No separator dots: a dot is its own node and gets orphaned at the start of the next
 * line on wrap. `gap-x-4` spaces the pairs without that problem.
 */
function MetaItem({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-baseline justify-between gap-2 sm:justify-start sm:gap-1.5">
      <dt className="shrink-0">{label}</dt>
      <dd className="min-w-0 truncate text-foreground/80">{children}</dd>
    </div>
  );
}

export function SessionRow({
  session,
  formatDate,
  onRevoke,
  isRevoking,
}: {
  session: Session;
  formatDate: (value: string | null | undefined) => string;
  onRevoke: (id: string) => void;
  isRevoking: boolean;
}) {
  const device = formatUserAgent(session.created_user_agent);
  const [confirming, setConfirming] = useState(false);

  return (
    /*
     * `flex-wrap` plus the revoke button's `w-full sm:w-auto`: full width on its own line
     * on phones (easy to tap), same line on desktop. Wrapping instead of writing the
     * button twice keeps one AlertDialog, so two dialogs are never mounted at once.
     */
    <li
      className={cn(
        "flex flex-wrap items-start gap-x-3 gap-y-3 rounded-lg border p-3",
        !session.active && "bg-muted/40"
      )}
    >
      <span className="mt-0.5 shrink-0 text-muted-foreground">
        <DeviceIcon userAgent={session.created_user_agent} />
      </span>

      {/* `basis-48` is the wrap threshold: with less than 12rem left the button
         drops to the next line. */}
      <div className="min-w-0 flex-1 basis-48 space-y-1.5">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          {/*
            If parsing fails the raw string is the title, and only when that is missing
            too (UA is null) the placeholder; `break-words` wraps long strings on phones
            instead of stretching the card. When parsing succeeds the raw string goes only
            in `title`. Touch devices cannot hover, but spotting "this isn't me" relies on
            the IP and time below; the raw UA is a last resort, and a line of its own
            would only make the card taller on phones.
          */}
          <span
            className="break-words font-medium text-sm"
            title={session.created_user_agent ?? undefined}
          >
            {device ??
              session.created_user_agent ??
              m.user_sessions_unknown_device()}
          </span>
          {session.current ? (
            <Badge variant="secondary">{m.user_sessions_current()}</Badge>
          ) : null}
          {session.active ? null : (
            <Badge variant="outline">
              {session.ended_reason
                ? endedReasonLabel(session.ended_reason)
                : m.user_sessions_ended_expired()}
            </Badge>
          )}
        </div>

        {/*
          No "expires" item. Whether a session is alive comes from `active`; showing an
          expiry invites comparing `expires_at` with the clock, and is easy to confuse
          with "last active". Revoke and the end-reason badge already say whether it
          still works.
        */}
        <dl className="space-y-0.5 text-muted-foreground text-xs sm:flex sm:flex-wrap sm:gap-x-4 sm:space-y-0">
          <MetaItem label={m.user_sessions_ip()}>
            {/* Without trusted_proxies the backend records no address, not a false one. */}
            <span className="font-mono">
              {session.created_ip ?? m.user_sessions_ip_unknown()}
            </span>
          </MetaItem>
          <MetaItem label={m.user_sessions_created()}>
            {formatDate(session.created_at)}
          </MetaItem>
          <MetaItem label={m.user_sessions_last_seen()}>
            {formatDate(session.last_seen_at)}
          </MetaItem>
        </dl>
      </div>

      {/* Revoke only for live sessions. `active` comes straight from the backend;
          expires_at is never compared with the local clock. */}
      {session.active && !session.current ? (
        <>
          <Button
            className="w-full sm:w-auto"
            disabled={isRevoking}
            onClick={() => setConfirming(true)}
            size="sm"
            type="button"
            variant="outline"
          >
            {m.user_sessions_revoke()}
          </Button>
          <ResponsiveDialog
            alert
            onOpenChange={setConfirming}
            open={confirming}
          >
            <ResponsiveDialogContent>
              <ResponsiveDialogHeader>
                <ResponsiveDialogTitle>
                  {m.user_sessions_revoke_confirm_title()}
                </ResponsiveDialogTitle>
                <ResponsiveDialogDescription>
                  {m.user_sessions_revoke_confirm_description()}
                </ResponsiveDialogDescription>
              </ResponsiveDialogHeader>
              <ResponsiveDialogFooter>
                <Button
                  onClick={() => setConfirming(false)}
                  type="button"
                  variant="outline"
                >
                  {m.common_cancel()}
                </Button>
                <Button
                  onClick={() => {
                    setConfirming(false);
                    onRevoke(session.id);
                  }}
                  type="button"
                >
                  {m.user_sessions_revoke()}
                </Button>
              </ResponsiveDialogFooter>
            </ResponsiveDialogContent>
          </ResponsiveDialog>
        </>
      ) : null}
    </li>
  );
}
